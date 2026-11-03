struct GiivBuffer { words: array<u32>, }
struct MapGame { flags: vec4<u32>, p: array<vec4<f32>, 16>, tiling: array<vec4<f32>, 12>, }
struct Layers {
    enabled: vec4<f32>,
    albedo_enabled: vec4<f32>,
    albedo_uv: vec4<f32>,
    normal_uv: vec4<f32>,
    normal_enabled: vec4<f32>,
    camera_fade: vec4<f32>,
    game: MapGame,
}
// LightSet values shared by every map material (MapLightBuffer).
struct MapLight {
    direct: array<vec4<f32>, 13>,
    hemi_axis: vec4<f32>,
    hemi_delta: vec4<f32>,
}
@if(BINDLESS) {
    struct LayerIndices {
        layers: u32,
        overlay: u32,
        overlay_sampler: u32,
        snow: u32,
        snow_sampler: u32,
        layer_c: u32,
        c_sampler: u32,
        overlay_normal: u32,
        overlay_normal_sampler: u32,
        snow_normal: u32,
        snow_normal_sampler: u32,
        c_normal: u32,
        c_normal_sampler: u32,
        height_mask: u32,
        height_sampler: u32,
        irradiance: u32,
        specular: u32,
        specular_sampler: u32,
        diffuse: u32,
        diffuse_sampler: u32,
        voxels: u32,
        voxels_sampler: u32,
        base_normal: u32,
        base_normal_sampler: u32,
        detail: u32,
        detail_sampler: u32,
        mask_l: u32,
        mask_l_sampler: u32,
        wet: u32,
        wet_sampler: u32,
        light: u32,
    }
    @group(constants::MATERIAL_BIND_GROUP) @binding(102) var<storage, read> giiv_buffers: binding_array<GiivBuffer>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(103) var<storage, read> light_buffers: binding_array<MapLight>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(100) var<storage> layer_indices: array<LayerIndices>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(101) var<storage> layer_array: array<Layers>;
}
@if(!BINDLESS) {
    @group(constants::MATERIAL_BIND_GROUP) @binding(50) var<uniform> layers: Layers;
    @group(constants::MATERIAL_BIND_GROUP) @binding(51) var overlay: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(52) var overlay_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(53) var snow: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(54) var snow_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(55) var layer_c: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(56) var c_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(57) var overlay_normal: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(58) var overlay_normal_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(59) var snow_normal: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(60) var snow_normal_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(61) var c_normal: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(62) var c_normal_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(63) var height_mask: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(64) var height_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(65) var<storage, read> giiv_buffer: GiivBuffer;
    @group(constants::MATERIAL_BIND_GROUP) @binding(66) var gi_specular: texture_cube_array<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(67) var gi_specular_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(68) var gi_diffuse: texture_cube_array<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(69) var gi_diffuse_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(70) var gi_voxels: texture_3d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(71) var gi_voxels_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(72) var base_normal: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(73) var base_normal_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(74) var detail: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(75) var detail_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(76) var mask_l: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(77) var mask_l_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(78) var wet: texture_2d<f32>;
    @group(constants::MATERIAL_BIND_GROUP) @binding(79) var wet_sampler: sampler;
    @group(constants::MATERIAL_BIND_GROUP) @binding(80) var<storage, read> map_light: MapLight;
}

