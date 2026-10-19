// The sky dome (sky.rs): camera-centred at infinite depth, the authored Ver2 / original
// SkyDome gradient, sun lobe and cloud masks, the Cloud layer and the FarMountain range.
// Adapted from sekiro-rs crates/game/src/sky.wgsl (9ee18b0; MIT / Apache-2.0). The Ver2 branch
// checked 2026-10-13 against the game's M[Sky]_SkyDome_Ver2_{0,1}_Fwd.ppo (same terms; colour
// packing rgb^2.2 x Intensity in sky.rs; switch 1 lights by the MTD's float3_6; the clouds'
// two-phase flow drift, `flow_sample`, 2026-10-13). gaps: the glow target (fog: the dome writes its depth, the fog pass covers it); the original SkyDome / Cloud branches and the star mix are not traced.

import bevy_pbr::render::{forward_io::{Vertex, VertexOutput}, mesh_view_bindings::view};

struct SkyInputs {
    colors: array<vec4<f32>,11>, floats: array<vec4<f32>,48>,
    vectors: array<vec4<f32>,10>, uv: array<vec4<f32>,10>, sun: vec4<f32>, control: vec4<f32>,
    flow_x: vec4<f32>,
}
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> sky: SkyInputs;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var high: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var high_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var low: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var low_sampler: sampler;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var o: VertexOutput;
    // Camera-centred at its real distance (the game's dome mesh, 3-6.4 km): the depth it writes
    // lets the fog pass fog it at that view depth, as the game's shader does.
    o.world_position=vec4(v.position,0.0);
    o.world_normal=normalize(v.position);
    o.position=view.clip_from_world*vec4(v.position+view.world_position,1.0);
    o.uv=v.uv;
    o.uv_b=v.uv_b;
    return o;
}
fn f(i: u32) -> f32 {return sky.floats[i].x;}
fn c(i: u32) -> vec3<f32> {return sky.colors[i].rgb;}
fn power(x: f32,p: f32) -> f32 {return pow(max(x,0.000001),max(p,0.000001));}

// The Ver2 clouds' drift (game asm, docs/kb/sky_shader.md): world +X in the (B, T, N) frame,
// x = its B part; each layer moves along U by 0.075 x that x strength, as a two-phase
// crossfade p = frac(t speed), frac(t speed + 0.5), weight |2 p0 - 1|, t = 2 x game time.
// The dome has no exported tangents, so T / B come from the screen derivatives of the
// position and uv0 (the shader's frame is the mesh's, built the same way from uv0).
fn flow_axis(p: vec3<f32>, uv: vec2<f32>) -> f32 {
    let dp1=dpdx(p); let dp2=dpdy(p); let du1=dpdx(uv); let du2=dpdy(uv);
    let n=normalize(cross(dp1,dp2));
    let det=du1.x*du2.y-du2.x*du1.y;
    if abs(det)<1e-12 {return 0.0;}
    let t=normalize((dp1*du2.y-dp2*du1.y)/det);
    let bv=(dp2*du1.x-dp1*du2.x)/det;
    let w=select(-1.0,1.0,dot(cross(n,t),bv)>=0.0);
    let b=normalize(cross(n,t)*w);
    let x=sky.flow_x.xyz;
    let v=vec3(dot(b,x),dot(t,x),dot(n,x));
    return v.x/max(length(v),1e-6);
}
fn flow_sample(tx: texture_2d<f32>, sm: sampler, uv: vec2<f32>, flow: f32, speed: f32) -> vec3<f32> {
    let t=2.0*sky.control.z*speed;
    let p0=fract(t); let p1=fract(t+0.5);
    let s0=textureSample(tx,sm,uv-vec2(flow*p0,0.0)).rgb;
    let s1=textureSample(tx,sm,uv-vec2(flow*p1,0.0)).rgb;
    return mix(s0,s1,abs(2.0*p0-1.0));
}

@fragment
fn fragment(v: VertexOutput) -> @location(0) vec4<f32> {
    let ray=normalize(v.world_position.xyz);
    let height=max(ray.y,0.0);
    let sun=max(dot(ray,sky.sun.xyz),0.0);
    var color: vec3<f32>;
    if sky.control.x>2.5 {
        // FarMountain (sekiro-rs sky.wgsl, after the game's M[LandScape]_FarMountain shader): the
        // painted RGBA range x colour 10 x a sun lobe between 13 and 12, blended toward a
        // height haze between 11 and 8 (1 - uv.y^1.5) by floats 21 / 22.
        let mountain=textureSample(high,high_sampler,v.uv);
        if mountain.a<sky.control.w { discard; }
        let lobe=min(power(sun*(1.0+f(23u)),f(24u)),1.0);
        let lit=mountain.rgb*c(0u)*mix(c(3u),c(2u),lobe);
        let haze=1.0-min(power(v.uv.y,1.5),1.0);
        let colour=mix(lit,mix(c(1u),c(4u),haze),mix(f(21u),f(22u),haze));
        return vec4(max(colour,vec3(0.0))*sky.control.y*view.exposure,1.0);
    }
    if sky.control.x>1.5 {
        // Native Cloud geometry and masks; normal perturbation is approximated by a mask slope.
        let drift=sky.control.z*0.0002;
        let a=textureSample(high,high_sampler,v.uv*sky.uv[0].xy+sky.uv[1].xy*drift).rgb;
        let b=textureSample(low,low_sampler,v.uv_b*sky.uv[3].xy+sky.uv[2].xy*drift).rgb;
        let mask=clamp(dot(a,vec3(0.333333))+dot(b,vec3(0.333333))-f(0u),0.0,1.0);
        let lit=mix(c(0u),c(1u),mask)+c(4u)*power(sun,max(f(5u),1.0))*mask;
        return vec4(lit*sky.control.y*view.exposure,mask);
    }
    if sky.control.x>0.5 {
        // Authored Ver2 gradient and sun lobe; colour slots carry their own intensity.
        let lobe=min(power(sun*(1.0+f(12u)),f(10u)),1.0);
        let sun_color=mix(c(3u),c(5u),lobe);
        let low_gradient=mix(c(6u),c(7u),min(power((1.0-height)*f(4u),f(6u)),1.0));
        let gradient=mix(c(0u),low_gradient,min(power((1.0-height)*(1.0+f(11u)),f(46u)),1.0));
        let clear=clamp(gradient+lobe*sun_color,vec3(0.0),vec3(1.0));
        let cover=1.0-min(power(height*f(36u),f(35u)),1.0);
        let fx=0.075*flow_axis(v.world_position.xyz,v.uv);
        let h=flow_sample(high,high_sampler,v.uv*sky.uv[6].xy,fx*f(38u),f(39u));
        let l=flow_sample(low,low_sampler,v.uv_b*sky.uv[7].xy+sky.uv[0].xy,fx*f(41u),f(40u));
        let mask=dot(l,sky.vectors[8].xyz);
        let high_mask=select(dot(h,sky.vectors[7].xyz),clamp(dot(h,sky.vectors[9].xyz),0.0,1.0),sky.control.w>0.5)+0.01;
        let rim_mask=dot(h,sky.vectors[5].xyz);
        let coverage=cover*mask;
        let edge=1.0-coverage;
        let rim=(max(1.0-2.0*abs(coverage-(1.0-4.0*coverage*f(7u))),0.0)+clamp(edge,0.0,1.0))*edge;
        let shade=clamp((edge-high_mask*f(20u)*(1.0-edge))*(1.0+f(21u)),0.0,1.0);
        let body=clamp((mask-high_mask*f(13u)*(1.0-mask))*(1.0+f(14u)),0.0,1.0);
        let coverage4=clamp(coverage*4.0,0.0,1.0);
        let cloud_color=mix(c(8u),mix(c(1u),c(4u),body),coverage4*coverage4*(3.0-2.0*coverage4));
        let illuminated=mix(cloud_color,shade*sun_color*c(2u)*rim,lobe*coverage4);
        let horizon_mask=clamp(height*3.0,0.0,1.0)*cover*rim_mask;
        let cloud=clamp(mix(horizon_mask,coverage,coverage4)*4.0*f(17u),0.0,1.0);
        color=mix(clear,illuminated,cloud);
        if sky.control.w>0.5 {
            let combined=mix(rim_mask,mask,cover);
            let corrected=clamp((combined-high_mask*f(13u)*(1.0-combined))*(1.0+f(14u)),0.0,1.0);
            let cloud_only=mix(c(1u),c(4u),corrected)*(1.0+f(9u));
            // Switch 1's lobe faces the MTD's float3_6, not the scene's light (game asm).
            let lobe1=min(power(max(dot(ray,normalize(sky.vectors[6].xyz+vec3(0.0,1e-6,0.0))),0.0)*(1.0+f(12u)),f(10u)),1.0);
            color=min(pow(max(cloud_only,vec3(0.000001)),vec3(max(f(8u),0.000001))),vec3(1.0))*mix(c(3u),c(5u),lobe1);
        }
    } else {
        // Original SkyDome branch: two height ramps and a directional lobe, mask clouds.
        let lobe=min(power(sun*(1.0+f(12u)),f(10u)),1.0);
        let h=mix(c(6u),c(7u),min(power(v.uv_b.y*f(4u),f(6u)),1.0));
        let gradient=mix(c(0u),h,min(power(v.uv_b.y*(1.0+f(11u)),f(6u)),1.0));
        let sun_color=mix(c(3u),c(5u),lobe);
        let mask=textureSample(high,high_sampler,v.uv*6.0).rgb;
        let clouds=clamp(dot(mask,sky.vectors[5].xyz)*f(7u),0.0,1.0);
        color=gradient+lobe*sun_color+clouds*sun_color;
        if sky.control.w>0.5 {
            // Star atlas stores independent layers in its channels, rather than RGB radiance.
            let star_a=textureSample(high,high_sampler,v.uv*f(0u)).g*c(1u);
            let star_b=textureSample(high,high_sampler,v.uv*f(1u)).g*c(8u);
            let selector=textureSample(high,high_sampler,v.uv*f(3u)).a
                *textureSample(high,high_sampler,v.uv*f(7u)).b;
            let stars=mix(mix(star_a,star_b,selector),
                c(3u)*textureSample(high,high_sampler,v.uv*f(2u)).b,
                textureSample(high,high_sampler,v.uv*f(5u)).r);
            let centered=v.uv+sky.uv[3].xy-vec2(0.5);
            let angle=f(37u);
            let rotated=vec2(centered.x*cos(angle)-centered.y*sin(angle),
                centered.x*sin(angle)+centered.y*cos(angle));
            let galaxy_mask=textureSample(low,low_sampler,rotated*sky.uv[1].xy+vec2(0.5)).g;
            let galaxy=textureSample(low,low_sampler,(v.uv-vec2(0.5))*f(16u)+vec2(0.5)).b;
            let weight=clamp(galaxy_mask*f(20u)*(1.0+f(21u)),0.0,1.0);
            color=h+stars+weight*(vec3(galaxy)+c(9u));
        }
    }
    // Match the scene's radiance bridge and camera exposure before native post metering.
    return vec4(max(color,vec3(0.0))*sky.control.y*view.exposure,1.0);
}
