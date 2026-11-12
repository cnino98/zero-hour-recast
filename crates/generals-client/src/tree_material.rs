//! Trees.nvv and `W3DTreeBuffer`'s alpha-tested, two-sided terrain pass.
use bevy::{
    asset::embedded_asset,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

pub struct TreeMaterialPlugin;
impl Plugin for TreeMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "tree_material.wgsl");
        app.add_plugins(MaterialPlugin::<TreeMaterial>::default());
    }
}

#[derive(Clone, ShaderType)]
pub struct Parameters {
    pub sway: [Vec4; 11],
    pub shroud_offset: Vec4,
    pub shroud_scale: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct TreeMaterial {
    #[uniform(0)]
    pub parameters: Parameters,
    #[texture(1)]
    #[sampler(2)]
    pub atlas: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub shroud: Option<Handle<Image>>,
}
impl Material for TreeMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://generals_client/tree_material.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(96.0 / 255.0)
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None::<Face>;
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = Some(true);
        }
        Ok(())
    }
}
