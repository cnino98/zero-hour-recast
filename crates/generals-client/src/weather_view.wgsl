#import bevy_pbr::{mesh_functions,forward_io::{Vertex,VertexOutput},view_transformations::position_world_to_clip}
struct Parameters {screen:u32,amount:f32}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> parameters:Parameters;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var particle_texture:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var particle_sampler:sampler;
@vertex fn vertex(input:Vertex)->VertexOutput {
    var output:VertexOutput;
    output.world_position=mesh_functions::mesh_position_local_to_world(mesh_functions::get_world_from_local(input.instance_index),vec4<f32>(input.position,1.0));
    output.position=position_world_to_clip(output.world_position.xyz);
    if(parameters.screen!=0u){output.position=vec4<f32>(input.position.xy,0.0,1.0);}
    output.world_normal=input.normal;output.uv=input.uv;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    output.instance_index=input.instance_index;
#endif
    return output;
}
@fragment fn fragment(input:VertexOutput)->@location(0) vec4<f32> {
    if(parameters.screen!=0u){return vec4<f32>(vec3<f32>(parameters.amount),1.0);}
    return textureSample(particle_texture,particle_sampler,input.uv);
}
