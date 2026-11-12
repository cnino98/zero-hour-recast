//! Encoded-color equivalents of original ps.1.1 water and alpha-only shore passes.
use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, ColorWrites,
        CompareFunction, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

#[derive(Clone, ShaderType)]
pub(super) struct Parameters {
    pub mode: u32,
    pub phase: f32,
    pub opacity: f32,
    pub padding: f32,
    pub extent: Vec4,
}

// Bevy's material asset and pipeline cache require these implementations.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(Key)]
pub(super) struct WaterMaterial {
    #[uniform(0)]
    pub parameters: Parameters,
    #[texture(1)]
    #[sampler(2)]
    pub water: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub sparkles: Option<Handle<Image>>,
    #[texture(5)]
    #[sampler(6)]
    pub noise: Option<Handle<Image>>,
    #[texture(7)]
    #[sampler(8)]
    pub edge: Option<Handle<Image>>,
    #[texture(9)]
    #[sampler(10)]
    pub shroud: Option<Handle<Image>>,
    #[texture(11)]
    #[sampler(12)]
    pub lookup: Option<Handle<Image>>,
    pub additive: bool,
    pub soft: bool,
    pub order: f32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct Key {
    mode: u32,
    additive: bool,
    soft: bool,
}
impl From<&WaterMaterial> for Key {
    fn from(material: &WaterMaterial) -> Self {
        Self {
            mode: material.parameters.mode,
            additive: material.additive,
            soft: material.soft,
        }
    }
}
impl Material for WaterMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://generals_client/water_view.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.parameters.mode == 0 {
            AlphaMode::Opaque
        } else {
            AlphaMode::Blend
        }
    }
    fn depth_bias(&self) -> f32 {
        self.order
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
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = Some(false);
            if key.bind_group_data.mode == 0 {
                depth.depth_compare = Some(CompareFunction::Always);
            }
        }
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                let key = &key.bind_group_data;
                target.write_mask = if key.mode < 2 {
                    ColorWrites::ALPHA
                } else {
                    ColorWrites::RED | ColorWrites::GREEN | ColorWrites::BLUE
                };
                target.blend = if key.mode < 2 {
                    None
                } else {
                    let source = if key.mode == 2 && key.soft {
                        BlendFactor::DstAlpha
                    } else if key.mode == 2 && key.additive {
                        BlendFactor::One
                    } else {
                        BlendFactor::SrcAlpha
                    };
                    let destination = if key.additive {
                        BlendFactor::One
                    } else if source == BlendFactor::DstAlpha {
                        BlendFactor::OneMinusDstAlpha
                    } else {
                        BlendFactor::OneMinusSrcAlpha
                    };
                    Some(BlendState {
                        color: BlendComponent {
                            src_factor: source,
                            dst_factor: destination,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent::REPLACE,
                    })
                };
            }
        }
        Ok(())
    }
}
