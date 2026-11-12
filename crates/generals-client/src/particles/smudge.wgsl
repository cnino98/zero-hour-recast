#import bevy_pbr::{mesh_functions, forward_io::{Vertex, VertexOutput}, view_transformations::position_world_to_clip, mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler}}
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> smudge: u32;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var color_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var color_sampler: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = vertex.normal;
    out.uv = vertex.uv;
    out.color = vertex.color;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}
@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    if smudge != 0u {
        let size = vec2<f32>(textureDimensions(view_transmission_texture));
        let source_uv = (input.uv * view.viewport.zw + view.viewport.xy) / size;
        let scene = textureSampleLevel(view_transmission_texture, view_transmission_sampler, source_uv, 0.0).rgb;
        // W3DSmudge ignores the copied framebuffer alpha and takes vertex alpha.
        return vec4<f32>(scene * input.color.rgb, input.color.a);
    }
    // The world target uses CompositingSpace::Srgb: source fixed-function
    // modulation and blending both operate on encoded RGB, including overlaps.
    return vec4<f32>(textureSample(color_texture, color_sampler, input.uv).rgb * input.color.rgb, 0.0);
}
