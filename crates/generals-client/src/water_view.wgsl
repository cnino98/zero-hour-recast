#import bevy_pbr::{mesh_functions, forward_io::{Vertex, VertexOutput}, view_transformations::position_world_to_clip}
struct Parameters { mode:u32, phase:f32, opacity:f32, padding:f32, extent:vec4<f32> }
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> parameters:Parameters;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var water:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var water_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var sparkles:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var sparkles_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var noise:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var noise_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var edge:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var edge_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var shroud:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var shroud_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var lookup:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var lookup_sampler:sampler;
@vertex fn vertex(input:Vertex)->VertexOutput {
    var output:VertexOutput;
    output.world_position=mesh_functions::mesh_position_local_to_world(mesh_functions::get_world_from_local(input.instance_index),vec4<f32>(input.position,1.0));
    output.position=position_world_to_clip(output.world_position.xyz);
    if(parameters.mode==0u){ output.position=vec4<f32>(input.position.xy,0.0,1.0); }
    output.world_normal=input.normal;
    output.uv=input.uv; output.uv_b=input.uv_b; output.color=input.color;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    output.instance_index=input.instance_index;
#endif
    return output;
}
@fragment fn fragment(input:VertexOutput)->@location(0) vec4<f32> {
    if(parameters.mode==0u){return vec4<f32>(0.0,0.0,0.0,parameters.opacity);}
    if(parameters.mode==1u){return textureSample(lookup,lookup_sampler,input.uv);}
    let base=textureSample(water,water_sampler,input.uv);
    // Both original shaders bind stage1 to the same coordinates as stage0.
    let highlights=textureSample(sparkles,sparkles_sampler,input.uv);
    let native=vec2<f32>(input.world_position.x,-input.world_position.z)*10.0;
    let detail=textureSample(noise,noise_sampler,native/16.0+vec2<f32>(parameters.phase));
    if(parameters.mode==3u){
        let bank=textureSample(edge,edge_sampler,input.uv_b);
        return vec4<f32>(base.rgb*input.color.rgb+(highlights.rgb*detail.rgb+bank.rgb)*input.color.a,base.a*bank.a);
    }
    var shade=vec3<f32>(1.0);
    if(parameters.extent.x>0.0){shade=textureSample(shroud,shroud_sampler,native*0.1/parameters.extent.xy).rgb;}
    return vec4<f32>((base.rgb*input.color.rgb+highlights.rgb*detail.rgb)*shade,base.a*input.color.a);
}
