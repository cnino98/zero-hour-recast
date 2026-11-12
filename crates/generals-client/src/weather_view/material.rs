//! Weather uses unlit original texture alpha; script flash uses encoded RGB.
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
    pub screen: u32,
    pub amount: f32,
}
#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(Key)]
pub(super) struct WeatherMaterial {
    #[uniform(0)]
    pub parameters: Parameters,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
    pub additive: bool,
}
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct Key {
    screen: bool,
    additive: bool,
}
impl From<&WeatherMaterial> for Key {
    fn from(value: &WeatherMaterial) -> Self {
        Self {
            screen: value.parameters.screen != 0,
            additive: value.additive,
        }
    }
}
impl Material for WeatherMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://generals_client/weather_view.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
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
            if key.bind_group_data.screen {
                depth.depth_compare = Some(CompareFunction::Always);
            }
        }
        let (source, destination) = if !key.bind_group_data.screen {
            (BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)
        } else if key.bind_group_data.additive {
            (BlendFactor::One, BlendFactor::One)
        } else {
            (BlendFactor::Zero, BlendFactor::Src)
        };
        if let Some(fragment) = &mut descriptor.fragment {
            for target in fragment.targets.iter_mut().flatten() {
                target.write_mask = ColorWrites::RED | ColorWrites::GREEN | ColorWrites::BLUE;
                target.blend = Some(BlendState {
                    color: BlendComponent {
                        src_factor: source,
                        dst_factor: destination,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent::REPLACE,
                });
            }
        }
        Ok(())
    }
}
