import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_view_bindings::globals;
import bevy_pbr::render::view_transformations::position_ndc_to_view;
import bevy_pbr::render::view_transformations::position_world_to_view;
@if(DEPTH_PREPASS)
import bevy_pbr::prepass::utils::prepass_depth;
@if(ENVIRONMENT_MAP)
import bevy_pbr::{
    light_probe::environment_map::environment_map_light,
    render::pbr_lighting::{LightingInput, LAYER_BASE, F_AB},
    render::clustered_forward::{view_fragment_cluster_index, unpack_clusterable_object_index_ranges},
};

struct Inputs {
    colors: array<vec4<f32>,4>, floats: array<vec4<f32>,14>, vectors: array<vec4<f32>,18>,
    uv: array<vec4<f32>,4>, control: vec4<f32>, sun: vec4<f32>, direct: vec4<f32>,
    specular: vec4<f32>, ambient: vec4<f32>, hemi_axis: vec4<f32>, hemi_delta: vec4<f32>,
}
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> fog: Inputs;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var mask: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var mask_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var flow: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var flow_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(5) var ground: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(6) var ground_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(7) var height: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(8) var height_sampler: sampler;
struct GiBuffer { words: array<u32>, }
@group(constants::MATERIAL_BIND_GROUP) @binding(9) var<storage,read> gi: GiBuffer;
@group(constants::MATERIAL_BIND_GROUP) @binding(10) var voxels: texture_3d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(11) var voxels_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(12) var diffuse: texture_cube_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(13) var diffuse_sampler: sampler;

fn gi_vec(index: u32) -> vec4<f32> {
    return bitcast<vec4<f32>>(vec4(gi.words[index],gi.words[index+1u],gi.words[index+2u],gi.words[index+3u]));
}
fn gi_sample(record: u32, uv: vec3<f32>, bias: vec3<f32>, axis: u32) -> vec3<f32> {
    let d=record+40u+axis*3u;
    let dimensions=vec3<f32>(vec3(gi.words[d],gi.words[d+1u],gi.words[d+2u]));
    let p=clamp(uv*dimensions+bias,vec3(0.5),dimensions-0.5);
    let origin=vec3<f32>(vec3(gi.words[record+20u]+axis*gi.words[record+23u],gi.words[record+21u],gi.words[record+22u]));
    let encoded=textureSampleLevel(voxels,voxels_sampler,(origin+p)/vec3<f32>(textureDimensions(voxels)),0.0);
    return 2.0*(encoded.rgb-0.5)*exp2(20.0*(encoded.a-0.5));
}
fn ambient_light(position: vec3<f32>, normal: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    var result=fallback;
    let n=vec3(-normal.x,normal.y,normal.z);
    // Use the shared map atlas and its authored local transforms; interpolate before decoding.
    for (var i=0u; i<gi.words[0]; i+=1u) {
        let r=4u+i*52u;
        if any(position<gi_vec(r+24u).xyz) || any(position>gi_vec(r+28u).xyz) { continue; }
        let transform=mat4x4(gi_vec(r),gi_vec(r+4u),gi_vec(r+8u),gi_vec(r+12u));
        let uv=(transform*vec4(position,1.0)).xyz;
        let dimensions=vec3<f32>(vec3(gi.words[r+16u],gi.words[r+17u],gi.words[r+18u]));
        let edge=min(uv,1.0-uv)*dimensions;
        let fade=smoothstep(0.0,1.0,min(edge.x,min(edge.y,edge.z)));
        if fade<=0.0 { continue; }
        var light: vec3<f32>;
        if gi.words[1]>0u {
            let bias=normalize((transform*vec4(normal,0.0)).xyz);
            light=max(n.x*gi_sample(r,uv,bias,0u)+n.y*gi_sample(r,uv,bias,1u)+n.z*gi_sample(r,uv,bias,2u)+gi_sample(r,uv,bias,3u),vec3(0.0));
        } else {
            light=textureSampleLevel(diffuse,diffuse_sampler,vec3(normal.xy,-normal.z),i32(gi.words[r+19u]),0.0).rgb;
        }
        result=mix(result,light*fog.control.w,fade);
    }
    return result;
}

fn f(i: u32) -> f32 { return fog.floats[i].x; }
fn sample_uv(v: VertexOutput, slot: u32, uv: vec2<f32>) -> vec2<f32> {
    var result=uv;
@if(VERTEX_UVS_B)
    if fog.uv[slot].z>0.5 { result+=v.uv_b-v.uv; }
    return result*fog.uv[slot].xy;
}
fn m(uv: vec2<f32>) -> vec3<f32> { return textureSample(mask,mask_sampler,uv).rgb; }

@fragment
fn fragment(v: VertexOutput) -> @location(0) vec4<f32> {
    let time=globals.time;
    var uv=v.uv;
    let n=normalize(v.world_normal);
    let toward=normalize(view.world_position-v.world_position.xyz);
    var coverage=1.0;
    var occlusion=1.0;
    @if(VERTEX_COLORS) {
        coverage=v.color.x;
        occlusion=v.color.w;
    }
    var albedo: vec3<f32>;
    var alpha: f32;
    var fade_scale: f32;
    if fog.control.x<0.5 {
        // CloudFog Fwd: four scrolling mask samples, native channel selections and bias.
        let a=textureSample(flow,flow_sampler,sample_uv(v,1u,uv+time*fog.vectors[10].xy)).r;
        let b=textureSample(flow,flow_sampler,sample_uv(v,1u,uv+time*fog.vectors[17].xy)).b;
        let c=dot(m(sample_uv(v,0u,uv+time*fog.vectors[15].xy)),fog.vectors[4].xyz);
        let d=dot(m(sample_uv(v,0u,uv+time*fog.vectors[16].xy)),fog.vectors[5].xyz);
        let density=clamp((a*b-(c*d+0.01)*f(12u)*(1.0-a*b))*(1.0+f(13u)),0.0,1.0);
        albedo=clamp(fog.colors[0].rgb+vec3(density),vec3(0.0),vec3(1.0));
        alpha=clamp(density*coverage,0.0,1.0);
        fade_scale=f(0u);
    } else {
        // GroundFog Fwd: flow warp, view-angle density, tangent parallax and static branch.
        let facing=clamp(dot(toward,n),0.0,1.0);
        let density=mix(f(1u),f(8u),facing);
        let a=textureSample(flow,flow_sampler,sample_uv(v,1u,uv+time*vec2(0.05,0.0)));
        let b=textureSample(flow,flow_sampler,sample_uv(v,1u,uv+time*vec2(0.0,0.05)));
        let warp=vec2(a.b+b.r,a.r+b.g)*f(2u);
        let base_mask=dot(m(sample_uv(v,0u,(uv+warp)*fog.vectors[10].xy)),fog.vectors[0].xyz);
        let base_color=mix(f(9u),1.0,base_mask)*fog.colors[2].rgb;
        var parallax=vec2(0.0);
        @if(VERTEX_TANGENTS) {
            let tangent=normalize(v.world_tangent.xyz);
            let bitangent=normalize(cross(n,tangent)*v.world_tangent.w);
            let ray=normalize(view.world_position-v.world_position.xyz);
            parallax=vec2(dot(ray,tangent),-dot(ray,bitangent));
        }
        let h=textureSample(height,height_sampler,sample_uv(v,3u,uv)).r-f(4u);
        let shifted=uv+parallax*h*f(3u);
        var surface: vec3<f32>;
        if fog.control.y>0.5 {
            surface=dot(m(sample_uv(v,0u,shifted*fog.vectors[11].xy)),fog.vectors[3].xyz)*fog.colors[3].rgb;
        } else {
            surface=textureSample(ground,ground_sampler,sample_uv(v,2u,shifted)).rgb*fog.colors[0].rgb;
        }
        let c=clamp(dot(m(sample_uv(v,0u,(uv+warp+time*fog.vectors[8].xy)*fog.vectors[6].xy)),fog.vectors[1].xyz),0.0,1.0);
        let d=clamp(dot(m(sample_uv(v,0u,(uv+warp+time*fog.vectors[9].xy)*fog.vectors[7].xy)),fog.vectors[2].xyz),0.0,1.0);
        albedo=clamp(mix(base_color,surface,c*d)*density,vec3(0.0),vec3(1.0));
        alpha=clamp(coverage,0.0,1.0);
        fade_scale=density*density;
    }
    albedo*=fog.colors[1].rgb;
    // Both families are lit diffuse surfaces with the native grazing specular term.
    let nl=clamp(dot(n,fog.sun.xyz),0.0,1.0);
    let nv=clamp(dot(n,toward),0.0,1.0);
    let half_vector=normalize(toward+fog.sun.xyz);
    let grazing=pow(1.0-clamp(dot(fog.sun.xyz,half_vector),0.0,1.0),5.0);
    let specular=grazing*nl/(0.5*max(max(nv,nl),0.0001)+0.5)*0.318310;
    var fallback=max(fog.ambient.rgb+fog.hemi_delta.rgb*dot(n,fog.hemi_axis.xyz),vec3(0.0));
    // The native no-volume path uses instance SH. Share the map renderer's default
    // environment-probe approximation instead of leaving far cards without sky lighting.
    @if(ENVIRONMENT_MAP) {
        var li: LightingInput;
        li.layers[LAYER_BASE].N=n;
        li.layers[LAYER_BASE].R=reflect(-toward,n);
        li.layers[LAYER_BASE].NdotV=max(nv,0.0001);
        li.layers[LAYER_BASE].perceptual_roughness=1.0;
        li.layers[LAYER_BASE].roughness=1.0;
        li.P=v.world_position.xyz;
        li.V=toward;
        li.diffuse_color=vec3(1.0);
        li.metallic=0.0;
        li.F0_dielectric=vec3(0.0);
        li.F0_metallic=vec3(0.0);
        li.F_ab=F_AB(1.0,max(nv,0.0001));
        var ranges=unpack_clusterable_object_index_ranges(view_fragment_cluster_index(v.position.xy,position_world_to_view(v.world_position.xyz).z,false));
        fallback+=environment_map_light(&li,&ranges,false).diffuse;
    }
    let ambient=ambient_light(v.world_position.xyz,n,fallback)*occlusion;
    let radiance=albedo*(ambient+fog.direct.rgb*nl)+fog.specular.rgb*specular;
    @if(DEPTH_PREPASS) {
        let scene_depth=prepass_depth(v.position,0u);
        let scene_z=-position_ndc_to_view(vec3(0.0,0.0,scene_depth)).z;
        let surface_z=-position_world_to_view(v.world_position.xyz).z;
        alpha*=clamp((scene_z-surface_z)*fade_scale,0.0,1.0);
    }
    @if(FOG_DIAGNOSTIC) {
        if alpha<=0.05 { discard; }
        let distance=length(v.world_position.xyz-view.world_position);
        let code=floor(clamp(log2(1.0+distance)/16.0,0.0,1.0)*65534.0)+1.0;
        let bytes=vec3(floor(code/256.0),code%256.0,0.0)/255.0;
        let linear=select(pow((bytes+vec3(0.055))/1.055,vec3(2.4)),bytes/12.92,bytes<=vec3(0.04045));
        return vec4(linear,1.0);
    }
@if(!FOG_DIAGNOSTIC)
    return vec4(radiance*view.exposure,alpha);
}
