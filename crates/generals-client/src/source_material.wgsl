#import bevy_pbr::{mesh_functions, forward_io::{Vertex,VertexOutput},view_transformations::position_world_to_clip}

struct Parameters {
    ambient:vec4<f32>,
    diffuse:vec4<f32>,
    emissive:vec4<f32>,
    scene_ambient:vec4<f32>,
    light_color:array<vec4<f32>,4>,
    light_direction:array<vec4<f32>,4>,
    point_position:array<vec4<f32>,4>,
    point_ambient:array<vec4<f32>,4>,
    terrain_lights:u32,
    alpha_test:f32,
    vertex_diffuse:u32,
    opacity:f32,
    primary_gradient:u32,
    shroud_extent:vec4<f32>,
    uv_transform:vec4<f32>,
    shroud_mode:u32,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material:Parameters;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var color_texture:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var color_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var shroud_texture:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var shroud_sampler:sampler;

@vertex
fn vertex(vertex:Vertex)->VertexOutput {
    var out:VertexOutput;
    let world_from_local=mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position=mesh_functions::mesh_position_local_to_world(world_from_local,vec4<f32>(vertex.position,1.0));
    out.position=position_world_to_clip(out.world_position.xyz);
    out.world_normal=mesh_functions::mesh_normal_local_to_world(vertex.normal,vertex.instance_index);
    out.uv=vertex.uv*material.uv_transform.xy+material.uv_transform.zw;
    var diffuse=vec3<f32>(0.0);
    var point_ambient=vec3<f32>(0.0);
    for(var index=0u;index<4u;index+=1u) {
        var direction=-material.light_direction[index].xyz;
        var attenuation=1.0;
        let point=material.point_position[index];
        if(point.w>0.0){
            let delta=point.xyz-out.world_position.xyz;
            let distance=length(delta)*10.0;
            direction=vec3<f32>(0.0);
            if(distance>0.0){direction=normalize(delta);}
            attenuation=0.0;
            if(distance<=point.w){
                let linear=material.light_direction[index].w;
                attenuation=1.0/(1.0+linear*distance+material.point_ambient[index].w*distance*distance);
            }
            point_ambient+=material.point_ambient[index].xyz*attenuation;
        }
        diffuse+=material.light_color[index].xyz*max(dot(normalize(out.world_normal),direction),0.0)*attenuation;
    }
    // W3DDisplay::setTimeOfDay installs zero specular for each directional light.
    // MeshMatDescClass::Configure_Material selects COLOR1 when the pass has DCG.
    let diffuse_color=select(material.diffuse,vertex.color,material.vertex_diffuse==1u);
    let lit=material.ambient.xyz*(material.scene_ambient.xyz+point_ambient)+diffuse_color.xyz*diffuse+material.emissive.xyz;
    out.color=vec4<f32>(clamp(lit,vec3<f32>(0.0),vec3<f32>(1.0)),diffuse_color.w);
    if(material.vertex_diffuse==0u){out.color.a*=vertex.color.a;}
    if(material.vertex_diffuse==2u){out.color=vec4<f32>(floor(out.color.rgb*vertex.color.rgb*255.0)/255.0,1.0);}
    // HeightMap::doTheDynamicLight packs the CPU-updated terrain vertex RGB.
    if(material.terrain_lights!=0u){out.color=vertex.color;}
    // W3D PointGroup/StreakRenderer use PRELIT_DIFFUSE vertex RGBA.
    // Terrain scorch also uses prelit RGBA, then receives the terrain shroud pass.
    if(material.vertex_diffuse>=3u){out.color=vertex.color;}
    // ShaderClass::Apply, GRADIENT_DISABLE with texturing: select texture RGBA.
    // Keep the drawable opacity override independent of material/vertex alpha.
    if(material.primary_gradient==0u){out.color=vec4<f32>(1.0);}
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index=vertex.instance_index;
#endif
    out.color.a*=material.opacity;
    return out;
}

@fragment
fn fragment(input:VertexOutput)->@location(0) vec4<f32> {
    var shroud=vec3<f32>(1.0);
    if(material.shroud_mode!=1u && material.shroud_extent.x>0.0){
        let point=vec2<f32>(input.world_position.x,-input.world_position.z);
        let uv=(point+material.shroud_extent.zw)/material.shroud_extent.xy;
        shroud=textureSample(shroud_texture,shroud_sampler,uv).rgb;
    }
    if(material.shroud_mode==2u){return vec4<f32>(shroud,1.0);}
    var color=textureSample(color_texture,color_sampler,input.uv)*input.color;
    if(color.a<material.alpha_test){discard;}
    if(material.shroud_mode==0u && material.vertex_diffuse!=3u){
        color=vec4<f32>(color.rgb*shroud,color.a);
    }
    // World cameras retain D3D8 encoded RGB through modulation and blending.
    // Bevy converts once when compositing that framebuffer into the display.
    return color;
}
