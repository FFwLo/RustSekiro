import sekiro_rs::{
    map_coverage::camera_coverage,
    map_coverage::lod_coverage,
};
import bevy_pbr::{
    render::forward_io::VertexOutput,
    render::forward_io::FragmentOutput,
    render::pbr_fragment::pbr_input_from_standard_material,
    render::pbr_functions::alpha_discard,
    render::pbr_functions::apply_pbr_lighting,
    render::pbr_functions::main_pass_post_lighting_processing,
    render::pbr_functions::calculate_tbn_mikktspace,
    render::pbr_functions::calculate_F0,
    render::pbr_functions::calculate_F0_dielectric,
    render::pbr_functions::calculate_diffuse_color,
    render::pbr_lighting::EnvBRDFApprox,
    render::pbr_lighting::F_AB,
    render::pbr_lighting::LightingInput,
    render::pbr_lighting::LAYER_BASE,
    render::pbr_lighting::perceptualRoughnessToRoughness,
    render::mesh_view_bindings::lights,
    render::mesh_view_bindings::view,
    render::pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    render::pbr_types::PbrInput,
    render::mesh_functions,
    render::view_transformations::position_world_to_clip,
    render::mesh_view_bindings::globals,
    render::pbr_bindings,
};
@if(ENVIRONMENT_MAP)
import bevy_pbr::light_probe::environment_map::environment_map_light;
import bevy_pbr::{
    render::clustered_forward::view_fragment_cluster_index,
    render::clustered_forward::unpack_clusterable_object_index_ranges,
    render::clustered_forward::get_clusterable_object_id,
    render::mesh_view_bindings::clustered_lights,
    render::mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    render::mesh_view_types::POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE,
    render::shadows::fetch_point_shadow,
    render::shadows::fetch_spot_shadow,
    render::pbr_lighting::derive_lighting_input,
    render::pbr_lighting::Fd_Burley,
    render::pbr_lighting::directional_light,
    render::pbr_lighting as native_lighting,
    render::shadows::fetch_directional_shadow,
    render::mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    render::mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
};
import sekiro_rs::{
    map_bindings::MapGame,
    map_bindings::Layers,
    map_bindings::MapLight,
};
@if(BINDLESS)
import bevy_render::bindless::{
    bindless_textures_2d,
    bindless_textures_3d,
    bindless_textures_cube_array,
    bindless_samplers_filtering,
};
@if(BINDLESS)
import bevy_pbr::{
    render::mesh_bindings::mesh,
    render::pbr_bindings::material_indices,
};
@if(BINDLESS)
import sekiro_rs::map_bindings::{
    layer_array,
    layer_indices,
    giiv_buffers,
    light_buffers,
};
@if(!BINDLESS)
import sekiro_rs::map_bindings::{
    layers,
    overlay,
    overlay_sampler,
    snow,
    snow_sampler,
    layer_c,
    c_sampler,
    overlay_normal,
    overlay_normal_sampler,
    snow_normal,
    snow_normal_sampler,
    c_normal,
    c_normal_sampler,
    height_mask,
    height_sampler,
    giiv_buffer,
    gi_specular,
    gi_specular_sampler,
    gi_diffuse,
    gi_diffuse_sampler,
    gi_voxels,
    gi_voxels_sampler,
    base_normal,
    base_normal_sampler,
    detail,
    detail_sampler,
    mask_l,
    mask_l_sampler,
    wet,
    wet_sampler,
    map_light,
};

struct GiScatter { FssEss: vec3<f32>, FmsEms: vec3<f32>, Edss: vec3<f32>, }
fn gi_multiscatter(f0: vec3<f32>, f_ab: vec2<f32>, missing: f32, occlusion: f32) -> GiScatter {
    let single = EnvBRDFApprox(f0,f_ab)*occlusion;
    let average = f0+(1.0-f0)/21.0;
    let multiple = single*average/(1.0-missing*average)*missing;
    return GiScatter(single,multiple,1.0-single-multiple);
}

fn gi_cube(slot: u32, index: u32, direction: vec3<f32>, mip: f32, diffuse: bool) -> vec3<f32> {
    let d = vec3(direction.xy,-direction.z);
    @if(BINDLESS) {
        if diffuse { return textureSampleLevel(bindless_textures_cube_array[layer_indices[slot].diffuse],bindless_samplers_filtering[layer_indices[slot].diffuse_sampler],d,i32(index),0.0).rgb; }
        return textureSampleLevel(bindless_textures_cube_array[layer_indices[slot].specular],bindless_samplers_filtering[layer_indices[slot].specular_sampler],d,i32(index),mip).rgb;
    }
    @if(!BINDLESS) {
        if diffuse { return textureSampleLevel(gi_diffuse,gi_diffuse_sampler,d,i32(index),0.0).rgb; }
        return textureSampleLevel(gi_specular,gi_specular_sampler,d,i32(index),mip).rgb;
    }
}

fn gi_word(slot: u32, index: u32) -> u32 {
@if(BINDLESS)
    return giiv_buffers[layer_indices[slot].irradiance].words[index];
@if(!BINDLESS)
    return giiv_buffer.words[index];
}

fn light_values(slot: u32) -> MapLight {
@if(BINDLESS)
    return light_buffers[layer_indices[slot].light];
@if(!BINDLESS)
    return map_light;
}

fn gi_vec(slot: u32, index: u32) -> vec4<f32> {
    return bitcast<vec4<f32>>(vec4(gi_word(slot,index), gi_word(slot,index+1u), gi_word(slot,index+2u), gi_word(slot,index+3u)));
}

// Native sampling interpolates the exponent-packed values BEFORE decoding.
fn gi_sample(slot: u32, record: u32, uv: vec3<f32>, bias: vec3<f32>, axis: u32) -> vec3<f32> {
    let d = record+40u+axis*3u;
    let dimensions = vec3(gi_word(slot,d),gi_word(slot,d+1u),gi_word(slot,d+2u));
    let sample_uv = uv+bias/vec3<f32>(dimensions);
    // Clamp to the region's voxel centres so filtering never touches another atlas tile.
    let p = clamp(sample_uv*vec3<f32>(dimensions),vec3(0.5),vec3<f32>(dimensions)-0.5);
    let origin = vec3(gi_word(slot,record+20u)+axis*gi_word(slot,record+23u),gi_word(slot,record+21u),gi_word(slot,record+22u));
    @if(BINDLESS) {
        let size = vec3<f32>(textureDimensions(bindless_textures_3d[layer_indices[slot].voxels]));
        let encoded = textureSampleLevel(bindless_textures_3d[layer_indices[slot].voxels],bindless_samplers_filtering[layer_indices[slot].voxels_sampler],(vec3<f32>(origin)+p)/size,0.0);
    }
    @if(!BINDLESS) {
        let size = vec3<f32>(textureDimensions(gi_voxels));
        let encoded = textureSampleLevel(gi_voxels,gi_voxels_sampler,(vec3<f32>(origin)+p)/size,0.0);
    }
    return 2.0*(encoded.rgb-0.5)*exp2(20.0*(encoded.a-0.5));
}

struct NativeAmbient { irradiance: vec3<f32>, lambert: vec3<f32>, specular: vec3<f32>, coverage: f32, }

fn gi_lighting(slot: u32, position: vec3<f32>, normal: vec3<f32>, reflection: vec3<f32>, mip: f32) -> NativeAmbient {
    var result: NativeAmbient;
    let n = vec3(-normal.x, normal.y, normal.z);
    var count = gi_word(slot,0u);
    var candidates = 0u;
    let grid = gi_word(slot,2u);
    if grid != 0u {
        let cell = floor((position-gi_vec(slot,grid).xyz)*gi_vec(slot,grid+4u).xyz);
        let dimensions = vec3(gi_word(slot,grid+8u),gi_word(slot,grid+9u),gi_word(slot,grid+10u));
        var entry = grid+12u;
        if all(cell>=vec3(0.0)) && all(cell<vec3<f32>(dimensions)) {
            let c = vec3<u32>(cell);
            entry = grid+16u+2u*(c.x+dimensions.x*(c.y+dimensions.y*c.z));
        }
        candidates = gi_word(slot,entry);
        count = gi_word(slot,entry+1u);
    }
    // Large enclosing boxes first; smaller authored regions fade over their parent.
    for (var i=0u; i<count; i+=1u) {
        var index = i;
        if grid != 0u { index = gi_word(slot,candidates+i); }
        let r = 4u+index*52u;
        if any(position<gi_vec(slot,r+24u).xyz) || any(position>gi_vec(slot,r+28u).xyz) { continue; }
        let transform = mat4x4(gi_vec(slot,r),gi_vec(slot,r+4u),gi_vec(slot,r+8u),gi_vec(slot,r+12u));
        let uv = (transform*vec4(position,1.0)).xyz;
        let dimensions = vec3(gi_word(slot,r+16u),gi_word(slot,r+17u),gi_word(slot,r+18u));
        let edge = min(uv,1.0-uv)*vec3<f32>(dimensions);
        let fade = smoothstep(0.0,1.0,min(edge.x,min(edge.y,edge.z)));
        if fade<=0.0 { continue; }
        if gi_word(slot,1u)>0u {
            // One native voxel of normal bias in local volume coordinates.
            let local_n = (transform*vec4(normal,0.0)).xyz;
            let bias = normalize(local_n);
            let x = gi_sample(slot,r,uv,bias,0u);
            let y = gi_sample(slot,r,uv,bias,1u);
            let z = gi_sample(slot,r,uv,bias,2u);
            let w = gi_sample(slot,r,uv,bias,3u);
            let light = max(n.x*x+n.y*y+n.z*z+w,vec3(0.0));
            result.irradiance = mix(result.irradiance,light,fade);
        }
        let cube = gi_word(slot,r+19u);
        if gi_word(slot,1u)==0u {
            result.lambert = mix(result.lambert,gi_cube(slot,cube,normal,0.0,true),fade);
        }
        // Intersect the receiver's reflected ray with the authored oriented box.
        // Env_Point supplies the capture origin; separate native PCC bounds remain untraced.
        let ray = (transform*vec4(reflection,0.0)).xyz;
        let safe_ray = select(vec3(0.0000001),ray,abs(ray)>vec3(0.0000001));
        let times = (select(vec3(0.0),vec3(1.0),safe_ray>vec3(0.0))-uv)/safe_ray;
        let t = min(times.x,min(times.y,times.z));
        let hit = uv+ray*t;
        var specular = vec3(0.0);
        if t>=0.0 && all(hit>=vec3(-0.0001)) && all(hit<=vec3(1.0001)) {
            let direction = position+reflection*t-gi_vec(slot,r+36u).xyz;
            if dot(direction,direction)>0.000001 { specular = gi_cube(slot,cube,direction,mip,false); }
        }
        result.specular = mix(result.specular,specular,fade);
        result.coverage += (1.0-result.coverage)*fade;
    }
    return result;
}

fn hemisphere_response(pbr: PbrInput) -> vec3<f32> {
    let nv = max(dot(pbr.N,pbr.V),0.0001);
    let diffuse = calculate_diffuse_color(pbr.material.base_color.rgb,pbr.material.metallic,pbr.material.specular_transmission,pbr.material.diffuse_transmission);
    let f0 = calculate_F0(pbr.material.base_color.rgb,pbr.material.metallic,pbr.material.reflectance);
    return EnvBRDFApprox(diffuse,F_AB(1.0,nv))
        + EnvBRDFApprox(f0,F_AB(pbr.material.perceptual_roughness,nv))*clamp(dot(f0,vec3(50.0*0.33)),0.0,1.0);
}

@if(NATIVE_DIRECT)
fn base_lighting_input(pbr: PbrInput) -> LightingInput {
    var li: LightingInput;
    let nv = max(dot(pbr.N,pbr.V),0.0001);
    li.layers[LAYER_BASE].N = pbr.N;
    li.layers[LAYER_BASE].R = reflect(-pbr.V,pbr.N);
    li.layers[LAYER_BASE].NdotV = nv;
    li.layers[LAYER_BASE].perceptual_roughness = pbr.material.perceptual_roughness;
    li.layers[LAYER_BASE].roughness = perceptualRoughnessToRoughness(pbr.material.perceptual_roughness);
    li.P = pbr.world_position.xyz;
    li.V = pbr.V;
    li.diffuse_color = calculate_diffuse_color(pbr.material.base_color.rgb,pbr.material.metallic,pbr.material.specular_transmission,pbr.material.diffuse_transmission);
    li.metallic = pbr.material.metallic;
    li.F0_dielectric = calculate_F0_dielectric(pbr.material.reflectance);
    li.F0_metallic = pbr.material.base_color.rgb;
    li.F_ab = F_AB(pbr.material.perceptual_roughness,nv);
    return li;
}

// Native deferred point/spot light (GXLightAcc_DPointLight1 / DSpotLight1 pixel shaders).
// With b = DiffColor.w = 1/falloff (builders 388130 / 3892e0 offset 0x4c: 1 / object +0x90),
// x = (d*b)^2 and fall = saturate(1 - d^2/R^2) (PositionAndRange.w = 1/R^2):
//   diffuse = Diff * fall * max(t^2+(1-t^2)*N.L,0) / (1+0.7x), t = 1.4/(1.4+d*b)
//   specular = Spec * F0 * 2.5/(1+x) * D * N.L / (max(N.V,N.L)*(1-a/2)+a/2) * fall
// FXR 609 lights carry the falloff distance (`unk_ds3_p2_0`) in Bevy's light radius; BTL lights
// have radius 0 and keep falloff = R, whose source field is not traced (#141).
// Spot lights also multiply by max(1-(tan/tan_outer)^2,0). Bevy's premultiplied colour is
// Diff * SCENE_TO_BEVY, so the result matches the directional path in native_direct.
// Not traced: TransLightParam (cb1[3], backlight) and the G-buffer translucency term, both 0 here.
@if(NATIVE_DIRECT)
fn native_clustered_light(light_id: u32, pbr: PbrInput, li: LightingInput, spot: bool) -> vec3<f32> {
    let light = &clustered_lights.data[light_id];
    let to_light = (*light).position_radius.xyz - pbr.world_position.xyz;
    let d2 = dot(to_light,to_light);
    var fall = saturate(1.0-d2*(*light).color_inverse_square_range.w);
    let falloff = (*light).position_radius.w;
    var x = d2*(*light).color_inverse_square_range.w;
    if falloff>0.0 { x = d2/(falloff*falloff); }
    if fall<=0.0 { return vec3(0.0); }
    let l = to_light*inverseSqrt(max(d2,1e-8));
    if spot {
        var axis = vec3<f32>((*light).light_custom_data.x,0.0,(*light).light_custom_data.y);
        axis.y = sqrt(max(0.0,1.0-axis.x*axis.x-axis.z*axis.z));
        if ((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE)!=0u { axis.y = -axis.y; }
        let cd = dot(-axis,l);
        if cd<=0.0 { return vec3(0.0); }
        let ratio = sqrt(max(1.0-cd*cd,0.0))/cd/max((*light).spot_light_tan_angle,1e-4);
        fall *= max(1.0-ratio*ratio,0.0);
        if fall<=0.0 { return vec3(0.0); }
    }
    let n = pbr.N;
    let nl_raw = dot(n,l);
    let t = 1.4/(1.4+sqrt(x));
    let t2 = t*t;
    let wrap = max(t2+(1.0-t2)*nl_raw,0.0);
    let diffuse = li.diffuse_color*(wrap*fall/(1.0+0.7*x)/3.14159265);
    let nv = max(dot(n,pbr.V),0.0001);
    let nl = saturate(nl_raw);
    let h = normalize(l+pbr.V);
    let nh = saturate(dot(n,h));
    let gloss = clamp(1.0-pbr.material.perceptual_roughness,0.0,1.0);
    let w = 10.0*(1.0+x);
    let e = exp2(13.0*gloss);
    let p = max(w*e/(w+e),1.0);
    let a = pow(1.0-0.95*sqrt(min(log2(p)/13.0,1.0)),2.0);
    let aa = a*a;
    let den = 1.0+nh*nh*(aa-1.0);
    let d = aa/(3.14159265*den*den);
    let vis = 1.0/(max(max(nv,nl),0.0001)*(1.0-a*0.5)+a*0.5);
    let f0 = calculate_F0(pbr.material.base_color.rgb,pbr.material.metallic,pbr.material.reflectance);
    // Native D already carries 1/pi (as in GXLightAcc_DDirLight1); the light colour is packed
    // with SCENE_TO_BEVY = pi / exposure, so specular takes the same 1/pi as diffuse (#7).
    let specular = f0*(2.5/(1.0+x)*d*nl*vis*fall/3.14159265);
    return (diffuse+specular)*(*light).color_inverse_square_range.rgb;
}

@if(NATIVE_DIRECT)
fn point_light_shadow(light_id: u32, pbr: PbrInput, spot: bool) -> f32 {
    let flags = clustered_lights.data[light_id].flags;
    if (pbr.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT)==0u || (flags & POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT)==0u { return 1.0; }
    if spot {
        return fetch_spot_shadow(light_id,pbr.world_position,pbr.world_normal,clustered_lights.data[light_id].shadow_map_near_z,pbr.frag_coord.xy);
    }
    return fetch_point_shadow(light_id,pbr.world_position,pbr.world_normal,pbr.frag_coord.xy);
}

// Replaces Bevy's inverse-square point/spot response with the native one: Bevy's own
// contribution (same inputs as apply_pbr_lighting's loops) is subtracted, the native one added.
@if(NATIVE_DIRECT)
fn native_point_lights(pbr: PbrInput, scale: f32) -> vec3<f32> {
    var li = base_lighting_input(pbr);
    let view_z = (view.view_from_world*pbr.world_position).z;
    var ranges = unpack_clusterable_object_index_ranges(view_fragment_cluster_index(pbr.frag_coord.xy,view_z,pbr.is_orthographic));
    var correction = vec3(0.0);
    for (var i=ranges.first_point_light_index_offset; i<ranges.first_reflection_probe_index_offset; i+=1u) {
        let spot = i>=ranges.first_spot_light_index_offset;
        let light_id = get_clusterable_object_id(i);
        var bevy = vec3(0.0);
        if spot { bevy = native_lighting::spot_light(light_id,&li,true); }
        else { bevy = native_lighting::point_light(light_id,&li,true,true); }
        correction += (native_clustered_light(light_id,pbr,li,spot)*scale-bevy)*point_light_shadow(light_id,pbr,spot);
    }
    return correction*view.exposure;
}

// Native four-slot colour routing with the recovered directional accumulation.
@if(NATIVE_DIRECT)
fn native_direct(pbr: PbrInput, light: MapLight) -> vec3<f32> {
    var li = base_lighting_input(pbr);
    let nv = max(dot(pbr.N,pbr.V),0.0001);
    var correction = vec3(0.0);
    var sun_shadow = 1.0;
    let view_z = (view.view_from_world*pbr.world_position).z;
    // MapLighting owns the scene's sun/fill DirectionalLights; point lights stay in Bevy.
    for (var i=0u; i<lights.n_directional_lights; i+=1u) {
        var shadow = 1.0;
        if (pbr.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT)!=0u && (lights.directional_lights[i].flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT)!=0u {
            shadow = fetch_directional_shadow(i,pbr.world_position,pbr.world_normal,view_z,pbr.frag_coord.xy);
            sun_shadow = shadow;
        }
        correction -= directional_light(i,&li,true)*shadow;
    }
    for (var i=0u; i<4u; i+=1u) {
        if light.direct[i*3u+2u].w<=0.0 { continue; }
        let diffuse = light.direct[i*3u+1u].rgb;
        let specular_rgb = light.direct[i*3u+2u].rgb;
        if all(diffuse==vec3(0.0)) && all(specular_rgb==vec3(0.0)) { continue; }
        var direction = light.direct[i*3u].xyz;
        // 310630 applies the same pi rotation then camera transform to a followed
        // fill and to both local slots. The world X mirror and camera -Z conversion
        // cancel that rotation into the packed direction used here.
        if light.direct[i*3u].w>0.0 { direction = (view.world_from_view*vec4(direction,0.0)).xyz; }
        var derived = derive_lighting_input(pbr.N,pbr.V,normalize(direction));
        let nl = max(dot(pbr.N,normalize(direction)),0.0);
        let h = normalize(pbr.V+normalize(direction));
        let nh = max(dot(pbr.N,h),0.0);
        let lh = max(dot(normalize(direction),h),0.0);
        // Native gloss is exported as 1 - perceptual roughness.
        let q = clamp(1.0-pbr.material.perceptual_roughness,0.0,1.0);
        let sharpness = max(light.direct[i*3u+1u].w,0.075);
        let a = pow(1.0-0.95*sqrt(q*(1.0-0.075/sharpness)),2.0);
        let aa = a*a;
        let d = aa/(3.14159265*pow(1.0+nh*nh*(aa-1.0),2.0));
        let f0 = calculate_F0(pbr.material.base_color.rgb,pbr.material.metallic,pbr.material.reflectance);
        let f = f0+(1.0-f0)*pow(1.0-lh,5.0);
        let vis = 1.0/(max(nl,nv)*(1.0-a*0.5)+a*0.5);
        var diffuse_angle = nl;
        if i>=2u { diffuse_angle = min(pow(0.5*dot(pbr.N,normalize(direction))+0.5,2.0),1.0); }
        // The packed colours carry SCENE_TO_BEVY = pi / exposure, so both terms take 1/pi;
        // native D (GXLightAcc_DDirLight1) already contains its own 1/pi, kept in d above (#7).
        correction += (li.diffuse_color*diffuse*diffuse_angle+f*d*nl*vis*specular_rgb)/3.14159265*select(1.0,sun_shadow,i==0u);
    }
    return correction*view.exposure;
}

fn sample_overlay(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
    return textureSample(bindless_textures_2d[layer_indices[slot].overlay], bindless_samplers_filtering[layer_indices[slot].overlay_sampler], uv);
@if(!BINDLESS)
    return textureSample(overlay, overlay_sampler, uv);
}

fn sample_snow(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
    return textureSample(bindless_textures_2d[layer_indices[slot].snow], bindless_samplers_filtering[layer_indices[slot].snow_sampler], uv);
@if(!BINDLESS)
    return textureSample(snow, snow_sampler, uv);
}

fn sample_layer_c(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
    return textureSample(bindless_textures_2d[layer_indices[slot].layer_c], bindless_samplers_filtering[layer_indices[slot].c_sampler], uv);
@if(!BINDLESS)
    return textureSample(layer_c, c_sampler, uv);
}

fn sample_overlay_normal(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
    return textureSample(bindless_textures_2d[layer_indices[slot].overlay_normal], bindless_samplers_filtering[layer_indices[slot].overlay_normal_sampler], uv);
@if(!BINDLESS)
    return textureSample(overlay_normal, overlay_normal_sampler, uv);
}

fn sample_snow_normal(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
    return textureSample(bindless_textures_2d[layer_indices[slot].snow_normal], bindless_samplers_filtering[layer_indices[slot].snow_normal_sampler], uv);
@if(!BINDLESS)
    return textureSample(snow_normal, snow_normal_sampler, uv);
}

fn sample_c_normal(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
    return textureSample(bindless_textures_2d[layer_indices[slot].c_normal], bindless_samplers_filtering[layer_indices[slot].c_normal_sampler], uv);
@if(!BINDLESS)
    return textureSample(c_normal, c_normal_sampler, uv);
}

fn sample_height_mask(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
    return textureSample(bindless_textures_2d[layer_indices[slot].height_mask], bindless_samplers_filtering[layer_indices[slot].height_sampler], uv);
@if(!BINDLESS)
    return textureSample(height_mask, height_sampler, uv);
}

fn layer_uv(in: VertexOutput, channel: f32) -> vec2<f32> {
@if(VERTEX_UVS_B)
    return mix(in.uv, in.uv_b, channel);
@if(!VERTEX_UVS_B)
    return in.uv;
}

// Endpoint-preserving height coverage, an approximation until native sat shaders are traced.
fn coverage(weight: f32, height: f32) -> f32 {
    if weight <= 0.0 { return 0.0; }
    if weight >= 1.0 { return 1.0; }
    return smoothstep(height - 0.25, height + 0.25, weight);
}

fn unpack_normal(sample: vec2<f32>) -> vec3<f32> {
    let xy = sample * 2.0 - 1.0;
    return normalize(vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.001))));
}

fn sample_base_normal(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
return textureSample(bindless_textures_2d[layer_indices[slot].base_normal], bindless_samplers_filtering[layer_indices[slot].base_normal_sampler], uv);
@if(!BINDLESS)
return textureSample(base_normal, base_normal_sampler, uv);
}
fn sample_detail(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
return textureSample(bindless_textures_2d[layer_indices[slot].detail], bindless_samplers_filtering[layer_indices[slot].detail_sampler], uv);
@if(!BINDLESS)
return textureSample(detail, detail_sampler, uv);
}
fn sample_mask_l(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
return textureSample(bindless_textures_2d[layer_indices[slot].mask_l], bindless_samplers_filtering[layer_indices[slot].mask_l_sampler], uv);
@if(!BINDLESS)
return textureSample(mask_l, mask_l_sampler, uv);
}
fn sample_wet(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
return textureSample(bindless_textures_2d[layer_indices[slot].wet], bindless_samplers_filtering[layer_indices[slot].wet_sampler], uv);
@if(!BINDLESS)
return textureSample(wet, wet_sampler, uv);
}
fn sample_base_albedo(uv: vec2<f32>, slot: u32) -> vec4<f32> {
@if(BINDLESS)
return textureSample(bindless_textures_2d[material_indices[slot].base_color_texture], bindless_samplers_filtering[material_indices[slot].base_color_sampler], uv);
@if(!BINDLESS)
return textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, uv);
}
@if(GAME_LAYERS)
struct MapVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
@if(VERTEX_UVS_A)
    @location(2) uv: vec2<f32>,
@if(VERTEX_UVS_B)
    @location(3) uv_b: vec2<f32>,
@if(VERTEX_TANGENTS)
    @location(4) tangent: vec4<f32>,
@if(VERTEX_COLORS)
    @location(5) color: vec4<f32>,
    @location(8) blend2: vec4<f32>,
}

@if(GAME_LAYERS)
struct MapVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
@if(VERTEX_UVS_A)
    @location(2) uv: vec2<f32>,
@if(VERTEX_UVS_B)
    @location(3) uv_b: vec2<f32>,
@if(VERTEX_TANGENTS)
    @location(4) world_tangent: vec4<f32>,
@if(VERTEX_COLORS)
    @location(5) color: vec4<f32>,
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    @location(6) @interpolate(flat) instance_index: u32,
@if(VISIBILITY_RANGE_DITHER)
    @location(7) @interpolate(flat) visibility_range_dither: i32,
    @location(8) blend2: vec4<f32>,
    @location(9) uv_scale: f32,
}

@if(GAME_LAYERS)
@vertex
fn vertex(v: MapVertex) -> MapVertexOutput {
    var out: MapVertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
@if(VERTEX_UVS_A)
    out.uv = v.uv;
@if(VERTEX_UVS_B)
    out.uv_b = v.uv_b;
@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, v.tangent, v.instance_index);
@if(VERTEX_COLORS)
    out.color = v.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = v.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, world_from_local[3]);
    out.blend2 = v.blend2;
    out.uv_scale = length(world_from_local[0].xyz);
    @if(!GAME_LAYERS) {
        out.blend2 = vec4(1.0, 1.0, 1.0, 0.0);
        out.uv_scale = 1.0;
    }
    return out;
}

// hb: the game's height blend of a vertex weight against a mask height.
fn hb(w: f32, h: f32, map_game: MapGame) -> f32 {
    return saturate((w - h * (1.0 - w)) * (1.0 + map_game.p[6].y));
}

// H: a mask's height for one layer: the material's float3 picks the channels, k scales it.
fn H(mask: vec3<f32>, sel: vec3<f32>, k: f32, map_game: MapGame) -> f32 {
    return (dot(mask, sel) * k + 0.01) * map_game.p[6].x;
}

// Wet exclusion precedes the height operation and final saturation in Ground facets.
fn snow_coverage(facing: f32, vertex_weight: f32, wet: f32, height: f32, sharpness: f32) -> f32 {
    let w = facing * vertex_weight * (1.0 - wet);
    return saturate((w - height * (1.0 - w)) * (1.0 + sharpness));
}

// FLV: a game normal map's xy (z rebuilt), DirectX green flipped into Bevy's tangent frame.
fn flv(n: vec4<f32>) -> vec3<f32> {
    let xy = vec2(n.r * 2.0 - 1.0, 1.0 - n.g * 2.0);
    return vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
}

fn layer_world_normal(nt: vec3<f32>, normal: vec3<f32>, tangent: vec4<f32>, is_front: bool) -> vec3<f32> {
    // Native FLVER normalizes each interpolated basis vector independently.
    // Only the geometric normal flips on a back face.
    let ng = normalize(normal);
    let n = select(-ng, ng, is_front);
    let t = normalize(tangent.xyz);
    let b = normalize(tangent.w * cross(ng, t));
    return normalize(nt.x * t + nt.y * b + nt.z * n);
}

fn has(bit: u32, map_game: MapGame) -> bool {
    return (map_game.flags.x & bit) != 0u;
}

@if(VERTEX_UVS_A)
fn game_layers(in: VertexOutput, b: vec4<f32>, b2: vec4<f32>, uv_scale: f32, is_front: bool, pbr_in: PbrInput, map_game: MapGame, slot: u32) -> PbrInput {
    var pbr_input = pbr_in;
    let p = map_game.p;
    if map_game.flags.y == 3u {
        // Native LandScape Mountain: normal-map B plus three authored mask projections.
        // UV tiling is FLVER texture scale, independent of the placed mesh's world scale.
        let mask_uv = layer_uv(in,p[13].z)*p[13].xy;
        let mask_a = sample_height_mask(mask_uv*p[4].xy,slot).rgb;
        let mask_b = sample_height_mask(mask_uv*p[3].xy,slot).rgb;
        let mask_c = sample_height_mask(mask_uv*p[5].xy,slot).rgb;
        let height = (dot(mask_a,p[6].xyz)+dot(mask_b,p[7].xyz)+0.01)*p[2].y;
        let normal_sample = sample_base_normal(layer_uv(in,p[9].z)*p[9].xy,slot);
        let coverage = 1.0-dot(mask_c,p[8].xyz)-normal_sample.b;
        let rock_weight = saturate((coverage-height*(1.0-coverage))*(1.0+p[2].z));
        let snow_color = sample_snow(layer_uv(in,p[10].z)*p[10].xy,slot).rgb*p[1].rgb;
        let rock_color = sample_base_albedo(layer_uv(in,p[11].z)*p[11].xy,slot).rgb*p[0].rgb;
        pbr_input.material.base_color = vec4(saturate(mix(snow_color,rock_color,rock_weight)),1.0);
        // COLOR1.x is the native baked ambient occlusion, separate from the snow mask.
        pbr_input.diffuse_occlusion *= b.x;
        pbr_input.specular_occlusion *= b.x;
        var nt = mix(vec3(0.0,0.0,1.0),flv(normal_sample),p[2].x);
        nt = vec3(nt.xy+flv(sample_detail(layer_uv(in,p[12].z)*p[12].xy,slot)).xy*p[2].w,nt.z);
        @if(VERTEX_TANGENTS) {
            let n = normalize(in.world_normal);
            let t = normalize(in.world_tangent.xyz);
            let bitangent = in.world_tangent.w*cross(n,t);
            pbr_input.N = normalize(nt.x*t+nt.y*bitangent+nt.z*n);
        }
        return pbr_input;
    }
    let ground = map_game.flags.y == 2u;
    let uv0 = in.uv;
    var uv1 = in.uv;
@if(VERTEX_UVS_B)
    uv1 = in.uv_b;
    let uvs = uv0 * uv_scale;
    var m9 = vec3(0.0);
    if has(128u, map_game) { m9 = sample_height_mask(uvs * map_game.tiling[8].xy, slot).rgb; }
    // M[Multiple]'s layer L / wet / tint mask (slot 6) is on uv1; M[MultipleGround] has none and
    // uses mask 9 for every layer.
    var m6 = m9;
    if has(512u, map_game) { m6 = sample_mask_l(uv1 * map_game.tiling[10].xy, slot).rgb; }

    var wOv = 0.0;
    if has(2u, map_game) { wOv = hb(b.x, H(m9, p[9].xyz, p[7].x, map_game), map_game); }
    var wL = 0.0;
    if has(32u, map_game) {
        if ground { wL = hb(b.y, H(m9, p[10].xyz, p[7].y, map_game), map_game); } else { wL = hb(b.y, H(m6, p[10].xyz, p[7].y, map_game), map_game); }
    }
    var wWet = 0.0;
    if has(1024u, map_game) {
        if ground { wWet = b2.w * p[7].z; } else { wWet = hb(b2.w, H(m6, p[11].xyz, p[7].z, map_game), map_game); }
    }
    let lum = dot(1.0 - b2.rgb, vec3(0.29891, 0.58661, 0.11448));
    // The vertex-colour tint is gated by mask 6, so variants without slot 6 (5, 10, 20, 25) take
    // no tint; their exported tint is often 0, which would turn the albedo black (#7).
    var wTint = 0.0;
    if has(512u, map_game) { wTint = hb(lum, H(m6, p[12].xyz, p[8].x, map_game), map_game); }

    // Normals: base / overlay (B gloss), the detail normal's xy added, layer L (ground).
    var nb = vec4(0.5, 0.5, 1.0, 0.15);
    if has(1u, map_game) { nb = sample_base_normal(uvs * map_game.tiling[1].xy, slot); }
    if has(4u, map_game) { nb = mix(nb, sample_overlay_normal(uvs * map_game.tiling[3].xy, slot), wOv); }
    if ground && has(64u, map_game) { nb = mix(nb, sample_c_normal(uvs * map_game.tiling[7].xy, slot), wL); }
    var nt = flv(nb);
    var gloss = nb.b;
    if has(256u, map_game) {
        let d = sample_detail(uv0 * map_game.tiling[9].xy, slot);
        var detail_xy = flv(d).xy * b.w;
        if !ground {
            // Multiple's native neutral is 0.502016; Ground multiplies decoded XY.
            detail_xy = flv(mix(vec4(0.502016, 0.502016, 1.0, 1.0), d, b.w)).xy;
        }
        nt = vec3(nt.xy + detail_xy * p[6].z, nt.z);
    }
    var Nw = pbr_input.world_normal;
@if(VERTEX_TANGENTS)
    Nw = layer_world_normal(nt, in.world_normal, in.world_tangent, is_front);
    // Snow: how much the bumped world normal faces the material's snow direction (not
    // normalised), times the vertex weight, height-blended on mask 9.
    var wSnow = 0.0;
    if has(8u, map_game) {
        wSnow = snow_coverage(dot(p[14].xyz, Nw), b.z, select(0.0, wWet, ground),
            H(m9, p[13].xyz, p[7].w, map_game), p[6].y);
    }
    // Multiple's flow normal is added before the replacement snow normal.
    if !ground && has(1024u, map_game) {
        let wn = sample_wet((uv1 + globals.time * p[15].xy) * map_game.tiling[11].xy, slot);
        nt = vec3(nt.xy + flv(wn).xy * p[6].w * wWet, nt.z);
        gloss = mix(gloss, p[8].y, wWet);
    }
    if wSnow > 0.0 && has(16u, map_game) {
        let sn = sample_snow_normal(uvs * map_game.tiling[5].xy, slot);
        if ground {
            nt = vec3(nt.xy + flv(sn).xy * p[9].w * wSnow, nt.z);
        } else {
            nt = mix(nt, flv(sn), wSnow);
        }
        gloss = mix(gloss, sn.b, wSnow);
    }
    // Ground's flow normal replaces the snow-composited normal afterwards.
    if ground && has(1024u, map_game) {
        let wn = sample_wet((uv1 + globals.time * p[15].xy) * map_game.tiling[11].xy, slot);
        nt = mix(nt, vec3(flv(wn).xy * p[6].w, 1.0), wWet);
        gloss = mix(gloss, p[8].y, wWet);
    }

    // Albedo.
    var a = vec3(1.0);
    if has(2048u, map_game) { a = sample_base_albedo(uvs * map_game.tiling[0].xy, slot).rgb; }
    var alpha = pbr_in.material.base_color.a;
    if ground {
        if has(2u, map_game) { a = mix(a, sample_overlay(uvs * map_game.tiling[2].xy, slot).rgb, wOv); }
        if has(32u, map_game) { a = mix(a, sample_layer_c(uvs * map_game.tiling[6].xy, slot).rgb, wL); }
        a = a * mix(vec3(1.0), b2.rgb, p[8].x);
    } else {
        a = a * mix(p[0].rgb, p[1].rgb, b.x);
        if has(2u, map_game) { a = mix(a, sample_overlay(uvs * map_game.tiling[2].xy, slot).rgb * p[2].rgb, wOv); }
        if has(32u, map_game) { a = mix(a, sample_layer_c((uv1 + p[8].zw) * map_game.tiling[6].xy, slot).rgb * p[3].rgb, wL); }
        a = mix(a, a * b2.rgb * p[4].rgb, wTint);
    }
    if has(8u, map_game) { a = mix(a, sample_snow(uvs * map_game.tiling[4].xy, slot).rgb * p[5].rgb, wSnow); }
    if ground { a = mix(a, p[4].rgb * mix(a, vec3(1.0), wWet * p[10].w), wWet); }
    a = saturate(a);
    if map_game.flags.z != 0u {
        // Debug (SHINOBI_MAP_SHOW_BLEND=1): overlay red, layer L green, snow blue.
        a = vec3(wOv, wL, wSnow);
    }
    pbr_input.material.base_color = vec4(a, alpha);
    pbr_input.material.perceptual_roughness = clamp(1.0 - saturate(gloss), 0.089, 1.0);
@if(VERTEX_TANGENTS)
    pbr_input.N = layer_world_normal(nt, in.world_normal, in.world_tangent, is_front);
    return pbr_input;
}

@fragment
fn fragment(
@if(GAME_LAYERS)
m: MapVertexOutput,
@if(!GAME_LAYERS)
vertex: VertexOutput,
@builtin(front_facing) is_front: bool) -> FragmentOutput {
    @if(BINDLESS && GAME_LAYERS) {
        var vertex: VertexOutput;
        vertex.position = m.position;
        vertex.world_position = m.world_position;
        vertex.world_normal = m.world_normal;
    }
@if(BINDLESS && GAME_LAYERS && VERTEX_UVS_A)
    vertex.uv = m.uv;
@if(BINDLESS && GAME_LAYERS && VERTEX_UVS_B)
    vertex.uv_b = m.uv_b;
@if(BINDLESS && GAME_LAYERS && VERTEX_TANGENTS)
    vertex.world_tangent = m.world_tangent;
@if(BINDLESS && GAME_LAYERS && VERTEX_COLORS)
    vertex.color = m.color;
@if(BINDLESS && GAME_LAYERS && VERTEX_OUTPUT_INSTANCE_INDEX)
    vertex.instance_index = m.instance_index;
@if(BINDLESS && GAME_LAYERS && VISIBILITY_RANGE_DITHER)
    vertex.visibility_range_dither = m.visibility_range_dither;
    @if(BINDLESS) {
        let slot = mesh[vertex.instance_index].material_and_lightmap_bind_group_slot & 0xffffu;
        let layers = layer_array[layer_indices[slot].layers];
    }
    @if(!BINDLESS && GAME_LAYERS) {
        var vertex: VertexOutput;
        vertex.position = m.position;
        vertex.world_position = m.world_position;
        vertex.world_normal = m.world_normal;
    }
@if(!BINDLESS && GAME_LAYERS && VERTEX_UVS_A)
    vertex.uv = m.uv;
@if(!BINDLESS && GAME_LAYERS && VERTEX_UVS_B)
    vertex.uv_b = m.uv_b;
@if(!BINDLESS && GAME_LAYERS && VERTEX_TANGENTS)
    vertex.world_tangent = m.world_tangent;
@if(!BINDLESS && GAME_LAYERS && VERTEX_COLORS)
    vertex.color = m.color;
@if(!BINDLESS && GAME_LAYERS && VERTEX_OUTPUT_INSTANCE_INDEX)
    vertex.instance_index = m.instance_index;
@if(!BINDLESS && GAME_LAYERS && VISIBILITY_RANGE_DITHER)
    vertex.visibility_range_dither = m.visibility_range_dither;
@if(!BINDLESS)
    let slot = 0u;
    let light = light_values(slot);
    var input = vertex;
@if(CAMERA_FADE)
    camera_coverage(input.position.xy, input.world_position.xyz, layers.camera_fade);
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    lod_coverage(input.position.xy, mesh_functions::get_tag(input.instance_index));
    var b = vec4(0.0);
@if(VERTEX_COLORS)
    b = input.color;
    var weights = vec3(0.0);
    @if(VERTEX_COLORS) {
        if layers.albedo_enabled.w > 0.0 {
            weights = input.color.rgb;
            // Exported layer colours carry weights; ordinary vertex tint is preserved.
            input.color = vec4(1.0);
        }
    }
    var pbr = pbr_input_from_standard_material(input, is_front);
@if(GAME_LAYERS)
    pbr = game_layers(input, b, m.blend2, m.uv_scale, is_front, pbr, layers.game, slot);
    @if(!GAME_LAYERS) {
        var height = 0.5;
        if layers.enabled.w > 0.0 {
            height = sample_height_mask(layer_uv(input, layers.albedo_uv.w), slot).r;
        }
        let facing = smoothstep(0.2, 0.7, normalize(input.world_normal).y);
        let w = vec3(coverage(weights.x, height), coverage(weights.y, 1.0-height) * facing, coverage(weights.z, height)) * layers.enabled.xyz;
        var color = pbr.material.base_color.rgb;
        if layers.albedo_enabled.x > 0.0 { color = mix(color, sample_overlay(layer_uv(input, layers.albedo_uv.x), slot).rgb, w.x); }
        if layers.albedo_enabled.z > 0.0 { color = mix(color, sample_layer_c(layer_uv(input, layers.albedo_uv.z), slot).rgb, w.z); }
        if layers.albedo_enabled.y > 0.0 { color = mix(color, sample_snow(layer_uv(input, layers.albedo_uv.y), slot).rgb, w.y); }
        pbr.material.base_color = vec4(color, pbr.material.base_color.a);
    }
    @if(!GAME_LAYERS && VERTEX_TANGENTS) {
        let tbn = calculate_tbn_mikktspace(input.world_normal, input.world_tangent);
        if layers.normal_enabled.x > 0.0 { pbr.N = normalize(mix(pbr.N, normalize(tbn * unpack_normal(sample_overlay_normal(layer_uv(input, layers.normal_uv.x), slot).rg)), w.x)); }
        if layers.normal_enabled.z > 0.0 { pbr.N = normalize(mix(pbr.N, normalize(tbn * unpack_normal(sample_c_normal(layer_uv(input, layers.normal_uv.z), slot).rg)), w.z)); }
        if layers.normal_enabled.y > 0.0 { pbr.N = normalize(mix(pbr.N, normalize(tbn * unpack_normal(sample_snow_normal(layer_uv(input, layers.normal_uv.y), slot).rg)), w.y)); }
    }
    pbr.material.base_color = alpha_discard(pbr.material.flags, pbr.material.alpha_cutoff, pbr.material.base_color);
    var out: FragmentOutput;
    let unlit = (pbr.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) != 0u;
    out.color = pbr.material.base_color;
    if !unlit { out.color = apply_pbr_lighting(pbr); }
    @if(NATIVE_DIRECT) {
        if !unlit && light.direct[12].x>0.0 {
            out.color = vec4(out.color.rgb+native_direct(pbr,light),out.color.a);
            if light.direct[12].y>0.0 {
                out.color = vec4(out.color.rgb+native_point_lights(pbr,light.direct[12].z),out.color.a);
            }
        }
    }
    if light.hemi_axis.w > 0.0 && !unlit {
        // Verified 310630 hemisphere: mean + dot(axis,N)*(up-down)/2.
        // Inferred bridge retains Bevy's ambient BRDF and replaces its flat colour.
        let mean = lights.ambient_color.rgb;
        let delta = light.hemi_delta.xyz;
        let response = hemisphere_response(pbr);
        // Evaluate the colour difference directly, avoiding a division by a dark mean.
        let irradiance = max(mean+dot(light.hemi_axis.xyz,pbr.N)*delta,vec3(0.0));
        out.color = vec4(out.color.rgb + response*(irradiance-mean)*pbr.diffuse_occlusion*view.exposure,out.color.a);
    }
    if !unlit {
        let gi = gi_lighting(slot,pbr.world_position.xyz,pbr.N,reflect(-pbr.V,pbr.N),min(6.5*pbr.material.perceptual_roughness,6.0));
        if gi.coverage>0.0 {
            let nv = max(dot(pbr.N,pbr.V),0.0001);
            let diffuse = calculate_diffuse_color(pbr.material.base_color.rgb,pbr.material.metallic,pbr.material.specular_transmission,pbr.material.diffuse_transmission);
            var fallback = vec3(0.0);
            var fallback_specular = vec3(0.0);
            @if(ENVIRONMENT_MAP) {
                var li: LightingInput;
                li.layers[LAYER_BASE].N = pbr.N;
                li.layers[LAYER_BASE].R = reflect(-pbr.V,pbr.N);
                li.layers[LAYER_BASE].NdotV = nv;
                li.layers[LAYER_BASE].perceptual_roughness = pbr.material.perceptual_roughness;
                li.layers[LAYER_BASE].roughness = perceptualRoughnessToRoughness(pbr.material.perceptual_roughness);
                li.P = pbr.world_position.xyz;
                li.V = pbr.V;
                li.diffuse_color = diffuse;
                li.metallic = pbr.material.metallic;
                li.F0_dielectric = calculate_F0_dielectric(pbr.material.reflectance);
                li.F0_metallic = pbr.material.base_color.rgb;
                li.F_ab = F_AB(pbr.material.perceptual_roughness,nv);
                let vz = (view.view_from_world*pbr.world_position).z;
                var ranges = unpack_clusterable_object_index_ranges(view_fragment_cluster_index(pbr.frag_coord.xy,vz,pbr.is_orthographic));
                let environment = environment_map_light(&li,&ranges,false);
                fallback = environment.diffuse;
                fallback_specular = environment.specular;
            }
            if light.hemi_axis.w>0.0 {
                // Remove the same response that the fallback adds, including its specular lobe.
                fallback += hemisphere_response(pbr)*max(lights.ambient_color.rgb+dot(light.hemi_axis.xyz,pbr.N)*light.hemi_delta.xyz,vec3(0.0));
            }
            let f_ab = F_AB(pbr.material.perceptual_roughness,nv);
            let f0 = calculate_F0(pbr.material.base_color.rgb,pbr.material.metallic,pbr.material.reflectance);
            let specular_occlusion = clamp(dot(f0,vec3(50.0*0.33)),0.0,1.0);
            let dielectric = gi_multiscatter(calculate_F0_dielectric(pbr.material.reflectance),f_ab,1.0-f_ab.x-f_ab.y,specular_occlusion);
            let metallic = gi_multiscatter(pbr.material.base_color.rgb,f_ab,1.0-f_ab.x-f_ab.y,specular_occlusion);
            let fss = mix(dielectric.FssEss,metallic.FssEss,pbr.material.metallic);
            let fms = mix(dielectric.FmsEms,metallic.FmsEms,pbr.material.metallic);
            var bounce = (fms+diffuse*dielectric.Edss)*gi.lambert;
            if gi_word(slot,1u)>0u { bounce = diffuse*gi.irradiance; }
            let delta_diffuse = bounce*light.hemi_delta.w-fallback*gi.coverage;
            let delta_specular = fss*gi.specular*light.hemi_delta.w-fallback_specular*gi.coverage;
            out.color = vec4(out.color.rgb+(delta_diffuse*pbr.diffuse_occlusion+delta_specular*pbr.specular_occlusion)*view.exposure,out.color.a);
        }
    }
    if layers.normal_enabled.w > 0.0 { out.color = vec4(weights, 1.0); }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
