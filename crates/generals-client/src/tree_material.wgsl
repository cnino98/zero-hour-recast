#import bevy_pbr::{forward_io::Vertex,view_transformations::position_world_to_clip}
struct Parameters {
    sway:array<vec4<f32>,11>,
    shroud_offset:vec4<f32>,
    shroud_scale:vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material:Parameters;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var shroud:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var shroud_sampler:sampler;
struct Output {
    @builtin(position) clip:vec4<f32>,
    @location(0) uv:vec2<f32>,
    @location(1) shroud_uv:vec2<f32>,
    @location(2) color:vec4<f32>,
}
@vertex
fn vertex(input:Vertex)->Output {
    var out:Output;
    // Source vertices and packed NORMAL data remain Z-up in this buffer.
    let displacement=(input.position.z-input.normal.z)*material.sway[u32(input.normal.x)].xyz;
    let point=input.position+displacement;
    out.clip=position_world_to_clip(vec3<f32>(point.x,point.z,-point.y)*0.1);
    out.uv=input.uv;
    out.color=vec4<f32>(input.color.rgb*input.normal.y,input.color.a);
    // Trees.nvv derives shroud UV from the unswayed position.
    out.shroud_uv=(input.position.xy+material.shroud_offset.xy)*material.shroud_scale.xy;
    return out;
}
@fragment
fn fragment(input:Output)->@location(0) vec4<f32> {
    var color=textureSample(atlas,atlas_sampler,input.uv)*input.color;
    if(color.a<96.0/255.0){discard;}
    if(material.shroud_scale.x!=0.0){color=vec4<f32>(color.rgb*textureSample(shroud,shroud_sampler,input.shroud_uv).rgb,color.a);}
    return color;
}
