// Map pieces (map.rs): the game's layered map materials (M[Multiple], M[MultipleGround],
// docs/kb/map.md "Layers") on top of Bevy's StandardMaterial. Bins of version 5 take the game's
// own blend (`game_layers`, decompiled); older bins the first reading below. Base albedo from the
// StandardMaterial; the overlay (moss, a second stone) and the snow come as extra textures and
// blend by the vertex blend bytes (vertex colour: x overlay, y layer C, z snow, w byte 2) and,
// for the snow, how much the surface faces up. Normal maps as sekiro_material.wgsl (RG, DirectX
// green, B = gloss).

import bevy_render::bindless::{bindless_textures_2d, bindless_textures_3d, bindless_samplers_filtering};
import bevy_pbr::render::{ mesh_view_bindings::{globals, view}, pbr_fragment::pbr_input_from_standard_material, pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing, calculate_diffuse_color}, pbr_types::{PbrInput, STANDARD_MATERIAL_FLAGS_UNLIT_BIT}, pbr_bindings, mesh_bindings::mesh, mesh_functions, view_transformations::position_world_to_clip, forward_io::{VertexOutput, FragmentOutput}, };

// Bindless (map.rs `MapExt`, after sekiro-rs 73eca1d): with BINDLESS each binding below is an
// entry of the material's index table and the uniforms one `MapUniform` in a storage array;
// `load_map` copies the uniforms into the private globals the rest of the file reads, and the
// `s_*` helpers sample either way.
// map_flags x: has a base normal map (bit 1: has layer C, bit 2: layer C with a normal), y: has
// an overlay (bit 1: with a normal), z: has snow (bit 1: with a normal), w: has a base albedo
// (bit 1: show the blend weights; bit 2: byte 2 darkens (AO); bit 3: byte 2 blends layer C).
// map_params x: overlay uv set (0 / 1; for flow smoke: the mask's tiling), y: snow uv set, z: snow
// up-facing start (N.y), w: snow up-facing full.
// mask_params x: has a mask, y: blend softness (0 = the plain vertex weight), z: flow smoke (the
// cloud sea, `smoke` below), w: its pace.
// mask: the base layer's _3m mask, R read as height (block faces high, mortar / plank gaps low).
// The game path (bin v5): the detail normal (M[Multiple] slot 0 / M[MultipleGround] 12), the
// layer mask on uv1 (M[Multiple] slot 6), the wet flow normal (slot 16 / 0); sampled with
// normal_sampler (every map texture repeats).
struct MapGame {
    // x: texture bits (map.rs GameParams), y: kind (0 the old path, 1 M[Multiple],
    // 2 M[MultipleGround]), z: show the blend weights.
    flags: vec4<u32>,
    p: array<vec4<f32>, 16>,
}
// One Env_Box's irradiance volume in the atlas (map.rs `GiivBox`).
struct GiivBox {
    // Rows of the Bevy point -> the box's 0..1 transform.
    uvw: array<vec4<f32>, 3>,
    // xyz: the box's metres along u, v, w; w: 1 / its fade distance.
    size: vec4<f32>,
    // xyz: its full voxel grid.
    grid: vec4<f32>,
    // xyz: the first voxel kept in the atlas (the exporter crops to the arena).
    lo: vec4<f32>,
    // xyz: the voxels kept; w: their first depth slice in the atlas (x, y, z, w volumes on).
    vox: vec4<f32>,
}
// The GI irradiance volumes of the Env_Boxes round the arena (map.rs `GiivParams`), in one RGBA8
// atlas as stored; giiv_next the variant after the hour.
struct Giiv {
    // Rows of the Bevy normal -> map-space normal turn (the coefficients' axes).
    nrm: array<vec4<f32>, 3>,
    // xyz: the atlas's texels, w: the box count (0: off).
    dims: vec4<f32>,
    // x: radiance scale (the probe cubes' EnvironmentMapLight intensity), y: the next
    // variant's weight (the hour between two).
    scale: vec4<f32>,
    // In drawing order (MSB priority).
    boxes: array<GiivBox, 16>,
}
struct MapUniform {
    mask_params: vec4<f32>,
    flags: vec4<u32>,
    params: vec4<f32>,
    game: MapGame,
    giiv: Giiv,
}
var<private> mask_params: vec4<f32>;
var<private> map_flags: vec4<u32>;
var<private> map_params: vec4<f32>;
var<private> map_game: MapGame;
var<private> giiv_nrm: array<vec4<f32>, 3>;
var<private> giiv_dims: vec4<f32>;
var<private> giiv_scale: vec4<f32>;
var<private> map_slot: u32;
var<private> map_data_index: u32;
@if(BINDLESS) {
struct MapIndices {
    data: u32,
    normal_texture: u32,
    normal_sampler: u32,
    over_texture: u32,
    over_sampler: u32,
    over_normal_texture: u32,
    over_normal_sampler: u32,
    snow_texture: u32,
    snow_sampler: u32,
    snow_normal_texture: u32,
    snow_normal_sampler: u32,
    c_texture: u32,
    c_sampler: u32,
    c_normal_texture: u32,
    c_normal_sampler: u32,
    mask_texture: u32,
    mask_sampler: u32,
    detail_texture: u32,
    mask_l_texture: u32,
    wet_texture: u32,
    giiv_texture: u32,
    giiv_sampler: u32,
    giiv_next_texture: u32,
}
@group(constants::MATERIAL_BIND_GROUP) @binding(123) var<storage> map_indices: array<MapIndices>;
@group(constants::MATERIAL_BIND_GROUP) @binding(124) var<storage> map_data: array<MapUniform>;
} @else {
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> map_data: MapUniform;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var normal_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var normal_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var over_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var over_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var over_normal_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var over_normal_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(107) var snow_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(108) var snow_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(109) var snow_normal_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(110) var snow_normal_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(111) var c_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(112) var c_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(113) var c_normal_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(114) var c_normal_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(115) var mask_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(116) var mask_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(117) var detail_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(118) var mask_l_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(119) var wet_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(120) var giiv_texture: texture_3d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(121) var giiv_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(122) var giiv_next_texture: texture_3d<f32>;
}

fn load_map(instance_index: u32) {
@if(BINDLESS) {
    map_slot = mesh[instance_index].material_and_lightmap_bind_group_slot & 0xffffu;
    map_data_index = map_indices[map_slot].data;
    let u = &map_data[map_data_index];
} @else {
    let u = &map_data;
}
    mask_params = (*u).mask_params;
    map_flags = (*u).flags;
    map_params = (*u).params;
    map_game = (*u).game;
    giiv_nrm = (*u).giiv.nrm;
    giiv_dims = (*u).giiv.dims;
    giiv_scale = (*u).giiv.scale;
}
// One box of the material's GI volume table, read in place (not copied per fragment).
fn giiv_box(i: u32) -> GiivBox {
@if(BINDLESS) {
    return map_data[map_data_index].giiv.boxes[i];
} @else {
    return map_data.giiv.boxes[i];
}
}

fn s_normal(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].normal_texture], bindless_samplers_filtering[map_indices[map_slot].normal_sampler], uv);
} @else {
    return textureSample(normal_texture, normal_sampler, uv);
}
}
fn s_over(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].over_texture], bindless_samplers_filtering[map_indices[map_slot].over_sampler], uv);
} @else {
    return textureSample(over_texture, over_sampler, uv);
}
}
fn s_over_normal(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].over_normal_texture], bindless_samplers_filtering[map_indices[map_slot].over_normal_sampler], uv);
} @else {
    return textureSample(over_normal_texture, over_normal_sampler, uv);
}
}
fn s_snow(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].snow_texture], bindless_samplers_filtering[map_indices[map_slot].snow_sampler], uv);
} @else {
    return textureSample(snow_texture, snow_sampler, uv);
}
}
fn s_snow_normal(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].snow_normal_texture], bindless_samplers_filtering[map_indices[map_slot].snow_normal_sampler], uv);
} @else {
    return textureSample(snow_normal_texture, snow_normal_sampler, uv);
}
}
fn s_c(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].c_texture], bindless_samplers_filtering[map_indices[map_slot].c_sampler], uv);
} @else {
    return textureSample(c_texture, c_sampler, uv);
}
}
fn s_c_normal(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].c_normal_texture], bindless_samplers_filtering[map_indices[map_slot].c_normal_sampler], uv);
} @else {
    return textureSample(c_normal_texture, c_normal_sampler, uv);
}
}
fn s_mask(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].mask_texture], bindless_samplers_filtering[map_indices[map_slot].mask_sampler], uv);
} @else {
    return textureSample(mask_texture, mask_sampler, uv);
}
}
fn s_detail(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].detail_texture], bindless_samplers_filtering[map_indices[map_slot].normal_sampler], uv);
} @else {
    return textureSample(detail_texture, normal_sampler, uv);
}
}
fn s_mask_l(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].mask_l_texture], bindless_samplers_filtering[map_indices[map_slot].normal_sampler], uv);
} @else {
    return textureSample(mask_l_texture, normal_sampler, uv);
}
}
fn s_wet(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[map_indices[map_slot].wet_texture], bindless_samplers_filtering[map_indices[map_slot].normal_sampler], uv);
} @else {
    return textureSample(wet_texture, normal_sampler, uv);
}
}
fn s_giiv(at: vec3<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSampleLevel(bindless_textures_3d[map_indices[map_slot].giiv_texture], bindless_samplers_filtering[map_indices[map_slot].giiv_sampler], at, 0.0);
} @else {
    return textureSampleLevel(giiv_texture, giiv_sampler, at, 0.0);
}
}
fn s_giiv_next(at: vec3<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSampleLevel(bindless_textures_3d[map_indices[map_slot].giiv_next_texture], bindless_samplers_filtering[map_indices[map_slot].giiv_sampler], at, 0.0);
} @else {
    return textureSampleLevel(giiv_next_texture, giiv_sampler, at, 0.0);
}
}
// The StandardMaterial's own base colour and albedo texture.
fn base_material_color() -> vec4<f32> {
@if(BINDLESS) {
    return pbr_bindings::material_array[pbr_bindings::material_indices[map_slot].material].base_color;
} @else {
    return pbr_bindings::material.base_color;
}
}
fn s_base(uv: vec2<f32>) -> vec4<f32> {
@if(BINDLESS) {
    return textureSample(bindless_textures_2d[pbr_bindings::material_indices[map_slot].base_color_texture], bindless_samplers_filtering[pbr_bindings::material_indices[map_slot].base_color_sampler], uv);
} @else {
    return textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, uv);
}
}

// One coefficient volume (axis 0..3 = x, y, z, w) of a box at its coordinates p, clamped to the
// centres of its voxels kept so the filter never reaches the next volume in the atlas. The encoded texel is
// filtered, then decoded: 2 (rgb - 0.5) 2^(20 (a - 0.5)) (the game's GXEnvMapRegion[Box][Diff]
// shader; sekiro-rs docs/SHADERS.md "GIIV").
fn giiv_axis(b: GiivBox, p: vec3<f32>, axis: f32) -> vec3<f32> {
    let d = b.vox.xyz;
    let q = clamp(p * b.grid.xyz - b.lo.xyz, vec3(0.5), d - 0.5);
    let at = vec3(q.x, q.y, b.vox.w + axis * d.z + q.z) / giiv_dims.xyz;
    let e = s_giiv(at);
    let a = 2.0 * (e.rgb - 0.5) * exp2(20.0 * (e.a - 0.5));
    // The next hour's variant, mixed after decoding (as the cubes' blend mixes radiance).
    let f = s_giiv_next(at);
    let c = 2.0 * (f.rgb - 0.5) * exp2(20.0 * (f.a - 0.5));
    return mix(a, c, giiv_scale.y);
}

// The Env_Box volumes' irradiance for a surface (rgb) and their cover (a), as the game's
// GXEnvMapRegion[Box][Diff] pass draws them one over another in priority order: a box weighs
// smoothstep(saturate(metres to its nearest face / its fade distance)) inside it, 0 outside,
// and blends over what the boxes before it gave. Each samples max(n.x X + n.y Y + n.z Z + W, 0)
// with n the map-space normal, one voxel out along the normal (sekiro-rs 6de8ff6
// map_material.wgsl gi_lighting; the game's shader steps along the normal too).
// gap: the box's FadeRate / DiffuseColorScale (MSB type data 0x28 / 0x2C?) are taken as 1.
fn giiv_light(P: vec3<f32>, N: vec3<f32>) -> vec4<f32> {
    let count = u32(giiv_dims.w);
    if count == 0u {
        return vec4(0.0);
    }
    let p4 = vec4(P, 1.0);
    let n = vec3(dot(giiv_nrm[0].xyz, N), dot(giiv_nrm[1].xyz, N), dot(giiv_nrm[2].xyz, N));
    var col = vec3(0.0);
    var cover = 0.0;
    for (var i = 0u; i < min(count, 16u); i++) {
        let b = giiv_box(i);
        let uvw = vec3(dot(b.uvw[0], p4), dot(b.uvw[1], p4), dot(b.uvw[2], p4));
        let m = min(uvw, 1.0 - uvw) * b.size.xyz;
        let inside = min(m.x, min(m.y, m.z));
        if inside < 0.0 {
            continue;
        }
        let w = smoothstep(0.0, 1.0, saturate(inside * b.size.w));
        if w <= 0.0 {
            continue;
        }
        let local = vec3(dot(b.uvw[0].xyz, N), dot(b.uvw[1].xyz, N), dot(b.uvw[2].xyz, N)) * b.grid.xyz;
        var p = uvw;
        if dot(local, local) > 1e-12 {
            p = uvw + normalize(local) / b.grid.xyz;
        }
        let light = max(n.x * giiv_axis(b, p, 0.0) + n.y * giiv_axis(b, p, 1.0) + n.z * giiv_axis(b, p, 2.0) + giiv_axis(b, p, 3.0), vec3(0.0));
        col = mix(col, light, w);
        cover = mix(cover, 1.0, w);
    }
    if cover <= 0.0 {
        return vec4(0.0);
    }
    return vec4(col / cover, cover);
}

// Bevy's lighting with the probe volume as the diffuse ambient where it covers the surface: the
// cube's diffuse (EnvironmentMapLight) fades out by the volume's weight and the volume's light
// comes in instead, on the same radiance scale and exposure; the cube's specular stays.
fn lit(pbr_in: PbrInput) -> vec4<f32> {
    var pbr_input = pbr_in;
    let gi = giiv_light(pbr_input.world_position.xyz, pbr_input.N);
    let occlusion = pbr_input.diffuse_occlusion;
    pbr_input.diffuse_occlusion = occlusion * (1.0 - gi.a);
    var color = apply_pbr_lighting(pbr_input);
    if gi.a > 0.0 {
        let diffuse = calculate_diffuse_color(pbr_input.material.base_color.rgb, pbr_input.material.metallic, pbr_input.material.specular_transmission, pbr_input.material.diffuse_transmission);
        color = vec4(color.rgb + diffuse * gi.rgb * gi.a * giiv_scale.x * occlusion * view.exposure, color.a);
    }
    return color;
}

// The vertex stage: Bevy's mesh.wgsl for static meshes plus the game path's extra vertex data
// (MAP_GAME_ATTRS when the mesh carries it: bin v5 pieces).
struct MapVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @if(VERTEX_UVS_A) @location(2) uv: vec2<f32>,
    @if(VERTEX_UVS_B) @location(3) uv_b: vec2<f32>,
    @if(VERTEX_TANGENTS) @location(4) tangent: vec4<f32>,
    @if(VERTEX_COLORS) @location(5) color: vec4<f32>,
    @if(MAP_GAME_ATTRS) @location(8) blend2: vec4<f32>,
    @if(MAP_GAME_ATTRS) @location(9) uv_scale: f32,
}

struct MapVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @if(VERTEX_UVS_A) @location(2) uv: vec2<f32>,
    @if(VERTEX_UVS_B) @location(3) uv_b: vec2<f32>,
    @if(VERTEX_TANGENTS) @location(4) world_tangent: vec4<f32>,
    @if(VERTEX_COLORS) @location(5) color: vec4<f32>,
    @if(VERTEX_OUTPUT_INSTANCE_INDEX) @location(6) @interpolate(flat) instance_index: u32,
    @if(VISIBILITY_RANGE_DITHER) @location(7) @interpolate(flat) visibility_range_dither: i32,
    @location(8) blend2: vec4<f32>,
    @location(9) uv_scale: f32,
}

@vertex
fn vertex(v: MapVertex) -> MapVertexOutput {
    var out: MapVertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
@if(VERTEX_UVS_A) {
    out.uv = v.uv;
}
@if(VERTEX_UVS_B) {
    out.uv_b = v.uv_b;
}
@if(VERTEX_TANGENTS) {
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, v.tangent, v.instance_index);
}
@if(VERTEX_COLORS) {
    out.color = v.color;
}
@if(VERTEX_OUTPUT_INSTANCE_INDEX) {
    out.instance_index = v.instance_index;
}
@if(VISIBILITY_RANGE_DITHER) {
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, world_from_local[3]);
}
@if(MAP_GAME_ATTRS) {
    out.blend2 = v.blend2;
    out.uv_scale = v.uv_scale;
} @else {
    out.blend2 = vec4(1.0, 1.0, 1.0, 0.0);
    out.uv_scale = 1.0;
}
    return out;
}

// A layer's coverage from its vertex weight and the base height: the layer wins where the
// weight exceeds the threshold, within `softness` of it. Overlays (moss, dirt) fill the low
// parts first (threshold = height), snow covers the high parts first (threshold = 1 - height).
fn coverage(weight: f32, threshold: f32) -> f32 {
    let s = mask_params.y;
    if s <= 0.0 {
        return weight;
    }
    // The threshold stays within [s, 1 - s]: weight 0 is nothing and weight 1 everything
    // whatever the height.
    let t = threshold * (1.0 - 2.0 * s) + s;
    return smoothstep(t - s, t + s, weight);
}

fn unpack_normal(n: vec4<f32>) -> vec3<f32> {
    var xy = n.rg * 2.0 - 1.0;
    xy.y = -xy.y; // DirectX green
    return vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}

// The Divine Dragon arena's cloud sea (M[Flow]_Smoke, map.rs alpha 3, kb/map.md): a stand-in.
// The game's shader scrolls a height / blend mask (mask_texture, m25_00_smoke_g) along a flow
// vector map (normal_texture, m25_00_SmokeFlow_v); here the mask is sampled at two phases of
// the flow half a cycle apart and cross-faded (the flow-map trick), drifted by the MTD's two
// float2 rates, and shaded between the MTD's shadow tint (color_3) and light tint (color_2, the
// base colour). gap: the SPX shader's own math (its floats 0..13, which uv set, the view fade).
fn smoke(in: VertexOutput, pbr_input: PbrInput) -> FragmentOutput {
    let t = globals.time * mask_params.w;
    // The mesh's uv spans -1..2 over the 490 m plane: the mask repeats map_params.x times per unit.
    let uv = in.uv * map_params.x;
    let flow = s_normal(in.uv).rg * 2.0 - 1.0;
    let cycle = 6.0;
    let p0 = fract(t / cycle);
    let p1 = fract(t / cycle + 0.5);
    let w = abs(2.0 * p0 - 1.0);
    let drift = vec2(-0.02, 0.01) * t * 0.05;
    let h0 = s_mask(uv + flow * p0 * 0.06 + drift).r;
    let h1 = s_mask(uv + flow * p1 * 0.06 + drift).r;
    // The mask is a soft grey cloud (R 0.61..0.88, mean 0.77): stretched to the full range.
    let h = smoothstep(0.62, 0.88, mix(h0, h1, w));
    let shadow = vec3(0.667, 0.706, 0.765);
    let light = base_material_color().rgb;
    let rgb = mix(shadow, light, h);
    let a = 0.92 + 0.08 * h;
    var input = pbr_input;
    input.material.base_color = vec4(rgb, a);
    var out: FragmentOutput;
    out.color = input.material.base_color;
    out.color = main_pass_post_lighting_processing(input, out.color);
    return out;
}

// ---------------------------------------------------------------------------------------------
// The game's own layer blend (bin version 5, kind 1 M[Multiple] / 2 M[MultipleGround]):
// decompiled from shader/shaderbdle.shaderbdlebnd.dcx M[Multiple].shaderbdle and
// M[MultipleGround].shaderbdle, Gbuf pass (docs/kb/map.md "The sat layer shaders, decompiled").
// The constants come from the material's MTD (`map_game.p`, layout in map.rs `GameParams`).
// Vertex data: color = (b0 overlay, b1 layer L, b4 snow, b5 detail strength), blend2 =
// (b2 tint rgb, b3 wet), uv_scale = |part scale X| (the game's uvS = uv0 x |world X axis|).

// hb: the game's height blend of a vertex weight against a mask height.
fn hb(w: f32, h: f32) -> f32 {
    return saturate((w - h * (1.0 - w)) * (1.0 + map_game.p[6].y));
}

// H: a mask's height for one layer: the material's float3 picks the channels, k scales it.
fn H(mask: vec3<f32>, sel: vec3<f32>, k: f32) -> f32 {
    return (dot(mask, sel) * k + 0.01) * map_game.p[6].x;
}

// FLV: a game normal map's xy (z rebuilt), DirectX green flipped into Bevy's tangent frame.
fn flv(n: vec4<f32>) -> vec3<f32> {
    return unpack_normal(n);
}

fn has(bit: u32) -> bool {
    return (map_game.flags.x & bit) != 0u;
}

@if(VERTEX_UVS_A) {
fn game_layers(in: VertexOutput, b: vec4<f32>, b2: vec4<f32>, uv_scale: f32, pbr_in: PbrInput) -> PbrInput {
    var pbr_input = pbr_in;
    let p = map_game.p;
    let ground = map_game.flags.y == 2u;
    let uv0 = in.uv;
    var uv1 = in.uv;
@if(VERTEX_UVS_B) {
    uv1 = in.uv_b;
}
    let uvs = uv0 * uv_scale;
    var m9 = vec3(0.0);
    if has(128u) { m9 = s_mask(uvs).rgb; }
    // M[Multiple]'s layer L / wet / tint mask (slot 6) is on uv1; M[MultipleGround] has none and
    // uses mask 9 for every layer.
    var m6 = m9;
    if has(512u) { m6 = s_mask_l(uv1).rgb; }

    var wOv = 0.0;
    if has(2u) { wOv = hb(b.x, H(m9, p[9].xyz, p[7].x)); }
    var wL = 0.0;
    if has(32u) {
        if ground { wL = hb(b.y, H(m9, p[10].xyz, p[7].y)); } else { wL = hb(b.y, H(m6, p[10].xyz, p[7].y)); }
    }
    var wWet = 0.0;
    if has(1024u) {
        if ground { wWet = saturate(b2.w * p[7].z); } else { wWet = hb(b2.w, H(m6, p[11].xyz, p[7].z)); }
    }
    let lum = dot(1.0 - b2.rgb, vec3(0.29891, 0.58661, 0.11448));
    let wTint = hb(lum, H(m6, p[12].xyz, p[8].x));

    // Normals: base / overlay (B gloss), the detail normal's xy added, layer L (ground).
    var nb = vec4(0.5, 0.5, 1.0, 0.15);
    if has(1u) { nb = s_normal(uvs); }
    if has(4u) { nb = mix(nb, s_over_normal(uvs), wOv); }
    if ground && has(64u) { nb = mix(nb, s_c_normal(uvs), wL); }
    var nt = flv(nb);
    var gloss = nb.b;
    if has(256u) {
        let d = mix(vec3(0.502, 0.502, 1.0), s_detail(uv0).rgb, b.w);
        nt = vec3(nt.xy + flv(vec4(d, 1.0)).xy * p[6].z, nt.z);
    }
    // Wet: a flow normal scrolled on uv1, gloss toward the wet gloss.
    if wWet > 0.0 {
        let wn = s_wet(uv1 + globals.time * p[15].xy);
        nt = vec3(nt.xy + flv(wn).xy * p[6].w * wWet, nt.z);
        gloss = mix(gloss, p[8].y, wWet);
    }
    var Nw = pbr_input.world_normal;
@if(VERTEX_TANGENTS) {
    let N0 = pbr_input.world_normal;
    let T = in.world_tangent.xyz;
    let B = in.world_tangent.w * cross(N0, T);
    Nw = normalize(nt.x * T + nt.y * B + normalize(nt).z * N0);
}
    // Snow: how much the bumped world normal faces the material's snow direction (not
    // normalised), times the vertex weight, height-blended on mask 9.
    var wSnow = 0.0;
    if has(8u) {
        let sd = dot(p[14].xyz, Nw) * b.z;
        wSnow = hb(sd, H(m9, p[13].xyz, p[7].w));
    }
    if wSnow > 0.0 && has(16u) {
        let sn = s_snow_normal(uvs);
        if ground {
            nt = vec3(nt.xy + flv(sn).xy * p[9].w * wSnow, nt.z);
        } else {
            nt = mix(nt, flv(sn), wSnow);
        }
        gloss = mix(gloss, sn.b, wSnow);
    }

    // Albedo.
    var a = vec3(1.0);
    if has(2048u) { a = s_base(uvs).rgb; }
    var alpha = base_material_color().a;
    if ground {
        if has(2u) { a = mix(a, s_over(uvs).rgb, wOv); }
        if has(32u) { a = mix(a, s_c(uvs).rgb, wL); }
        a = a * mix(vec3(1.0), b2.rgb, p[8].x);
        // gap: M[MultipleGround] wet darkening read as a lerp toward color_1 (kb/map.md).
        a = mix(a, p[4].rgb, wWet * p[10].w);
    } else {
        a = a * mix(p[0].rgb, p[1].rgb, b.x);
        if has(2u) { a = mix(a, s_over(uvs).rgb * p[2].rgb, wOv); }
        if has(32u) { a = mix(a, s_c(uv1 + p[8].zw).rgb * p[3].rgb, wL); }
        a = mix(a, a * b2.rgb * p[4].rgb, wTint);
    }
    if has(8u) { a = mix(a, s_snow(uvs).rgb * p[5].rgb, wSnow); }
    a = saturate(a);
    if map_game.flags.z != 0u {
        // Debug (SHINOBI_MAP_SHOW_BLEND=1): overlay red, layer L green, snow blue.
        a = vec3(wOv, wL, wSnow);
    }
    pbr_input.material.base_color = vec4(a, alpha);
    pbr_input.material.perceptual_roughness = clamp(1.0 - saturate(gloss), 0.089, 1.0);
@if(VERTEX_TANGENTS) {
    var mapped = normalize(nt.x * T + nt.y * B + nt.z * N0);
    if dot(N0, pbr_input.V) < 0.0 {
        mapped = mapped - 2.0 * dot(mapped, N0) / dot(N0, N0) * N0;
    }
    pbr_input.N = mapped;
}
    return pbr_input;
}
}

@fragment
fn fragment(m: MapVertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in: VertexOutput;
    in.position = m.position;
    in.world_position = m.world_position;
    in.world_normal = m.world_normal;
@if(VERTEX_UVS_A) {
    in.uv = m.uv;
}
@if(VERTEX_UVS_B) {
    in.uv_b = m.uv_b;
}
@if(VERTEX_TANGENTS) {
    in.world_tangent = m.world_tangent;
}
@if(VERTEX_COLORS) {
    in.color = m.color;
}
@if(VERTEX_OUTPUT_INSTANCE_INDEX) {
    in.instance_index = m.instance_index;
}
@if(VISIBILITY_RANGE_DITHER) {
    in.visibility_range_dither = m.visibility_range_dither;
}
    load_map(m.instance_index);
    var pbr_input = pbr_input_from_standard_material(in, is_front);
@if(VERTEX_UVS_A) {
    if map_game.flags.y != 0u {
        var b = vec4(0.0, 0.0, 0.0, 1.0);
@if(VERTEX_COLORS) {
        b = in.color;
}
        pbr_input = game_layers(in, b, m.blend2, m.uv_scale, pbr_input);
        pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);
        var out: FragmentOutput;
        if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
            out.color = lit(pbr_input);
        } else {
            out.color = pbr_input.material.base_color;
        }
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }
}

@if(VERTEX_UVS_A) {
    if mask_params.z > 0.5 {
        return smoke(in, pbr_input);
    }
    var uv_over = in.uv;
    var uv_snow = in.uv;
@if(VERTEX_UVS_B) {
    if map_params.x > 0.5 { uv_over = in.uv_b; }
    if map_params.y > 0.5 { uv_snow = in.uv_b; }
}
    var blend = vec4(0.0);
@if(VERTEX_COLORS) {
    blend = in.color;
}
    // Base colour: the StandardMaterial's albedo without the vertex colour Bevy multiplied in.
    var color = pbr_input.material.base_color;
    if map_flags.w != 0u {
        color = base_material_color() * s_base(in.uv);
    } else {
        color = base_material_color();
    }
    var height = 0.5;
    if mask_params.x > 0.5 {
        height = s_mask(in.uv).r;
    }
    var nt = vec3(0.0, 0.0, 1.0);
    var gloss = 0.15;
    if (map_flags.x & 1u) != 0u {
        let n = s_normal(in.uv);
        nt = unpack_normal(n);
        gloss = n.b;
    }
    if (map_flags.w & 4u) != 0u {
        color = vec4(color.rgb * blend.w, color.a);
    }
    if (map_flags.x & 2u) != 0u && (map_flags.w & 8u) != 0u && blend.w > 0.0 {
        let c = s_c(in.uv);
        let w = coverage(blend.w, height) * c.a;
        color = vec4(mix(color.rgb, c.rgb, w), color.a);
        if (map_flags.x & 4u) != 0u {
            let n = s_c_normal(in.uv);
            nt = mix(nt, unpack_normal(n), w);
            gloss = mix(gloss, n.b, w);
        }
    }
    if map_flags.y != 0u && blend.x > 0.0 {
        let o = s_over(uv_over);
        let w = coverage(blend.x, height) * o.a;
        color = vec4(mix(color.rgb, o.rgb, w), color.a);
        if (map_flags.y & 2u) != 0u {
            let n = s_over_normal(uv_over);
            nt = mix(nt, unpack_normal(n), w);
            gloss = mix(gloss, n.b, w);
        }
    }
    if map_flags.z != 0u && blend.z > 0.0 {
        // Snow settles on surfaces that face up.
        let up = smoothstep(map_params.z, map_params.w, pbr_input.world_normal.y);
        let w = coverage(blend.z * up, 1.0 - height);
        if w > 0.0 {
            let sn = s_snow(uv_snow);
            color = vec4(mix(color.rgb, sn.rgb, w), color.a);
            if (map_flags.z & 2u) != 0u {
                let n = s_snow_normal(uv_snow);
                nt = mix(nt, unpack_normal(n), w);
                gloss = mix(gloss, n.b, w);
            }
        }
    }
    if (map_flags.w & 2u) != 0u {
        // Debug: the blend weights (SHINOBI_MAP_SHOW_BLEND=1).
        color = vec4(blend.x, blend.z, blend.w, 1.0);
    }
    pbr_input.material.base_color = color;
    pbr_input.material.perceptual_roughness = clamp(1.0 - gloss, 0.089, 1.0);
@if(VERTEX_TANGENTS) {
    if map_flags.x != 0u || map_flags.y != 0u || map_flags.z != 0u {
        let N = pbr_input.world_normal;
        let T = in.world_tangent.xyz;
        let B = in.world_tangent.w * cross(N, T);
        var mapped = normalize(nt.x * T + nt.y * B + nt.z * N);
        if dot(N, pbr_input.V) < 0.0 {
            mapped = mapped - 2.0 * dot(mapped, N) / dot(N, N) * N;
        }
        pbr_input.N = mapped;
    }
}
}
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = lit(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
