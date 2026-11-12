//! Selected W3D fixed-function material pass, using the original per-vertex light equation.
use bevy::{
    asset::embedded_asset,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, CompareFunction,
        Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

pub struct SourceMaterialPlugin;

impl Plugin for SourceMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "source_material.wgsl");
        app.add_plugins(MaterialPlugin::<SourceMaterial>::default());
        crate::stealth_view::register(app);
    }
}

#[derive(Clone, ShaderType, PartialEq)]
pub struct Parameters {
    pub ambient: Vec4,
    pub diffuse: Vec4,
    pub emissive: Vec4,
    pub scene_ambient: Vec4,
    pub light_color: [Vec4; 4],
    pub light_direction: [Vec4; 4],
    pub point_position: [Vec4; 4],
    pub point_ambient: [Vec4; 4],
    pub terrain_lights: u32,
    pub alpha_test: f32,
    pub vertex_diffuse: u32,
    pub opacity: f32,
    pub primary_gradient: u32,
    pub shroud_extent: Vec4,
}

// Asset/TypePath/AsBindGroup are Bevy registration requirements; Material requires Clone.
#[derive(Asset, TypePath, AsBindGroup, Clone, PartialEq)]
#[bind_group_data(MaterialKey)]
pub struct SourceMaterial {
    #[uniform(0)]
    pub parameters: Parameters,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub shroud: Option<Handle<Image>>,
    pub alpha_mode: AlphaMode,
    pub force_sorted_phase: bool,
    pub double_sided: bool,
    pub depth_bias: f32,
    pub depth_write: bool,
    pub depth_compare: Option<CompareFunction>,
}

// Pipeline specialization keys must be cloneable, hashable and comparable.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MaterialKey {
    double_sided: bool,
    blend_source: Option<BlendFactor>,
    force_sorted_phase: bool,
    depth_write: bool,
    depth_compare: Option<CompareFunction>,
}

impl From<&SourceMaterial> for MaterialKey {
    fn from(value: &SourceMaterial) -> Self {
        Self {
            double_sided: value.double_sided,
            blend_source: match value.alpha_mode {
                AlphaMode::Add => Some(BlendFactor::One),
                AlphaMode::Blend => Some(BlendFactor::SrcAlpha),
                AlphaMode::Multiply => Some(BlendFactor::Zero),
                _ => None,
            },
            force_sorted_phase: value.force_sorted_phase,
            depth_write: value.depth_write,
            depth_compare: value.depth_compare,
        }
    }
}

impl Material for SourceMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://generals_client/source_material.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.force_sorted_phase {
            AlphaMode::Blend
        } else {
            self.alpha_mode
        }
    }
    fn depth_bias(&self) -> f32 {
        self.depth_bias
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
        descriptor.primitive.cull_mode = if key.bind_group_data.double_sided {
            None
        } else {
            Some(Face::Back)
        };
        if let Some(depth) = &mut descriptor.depth_stencil {
            depth.depth_write_enabled = Some(key.bind_group_data.depth_write);
            if let Some(compare) = key.bind_group_data.depth_compare {
                depth.depth_compare = Some(compare);
            }
        }
        if let Some(source) = key.bind_group_data.blend_source
            && let Some(fragment) = &mut descriptor.fragment
        {
            let blend = blend_component(source);
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState {
                    color: blend,
                    alpha: blend,
                });
            }
        }
        if key.bind_group_data.force_sorted_phase
            && key.bind_group_data.blend_source.is_none()
            && let Some(fragment) = &mut descriptor.fragment
        {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = None;
            }
        }
        Ok(())
    }
}

impl SourceMaterial {
    pub fn new(
        ambient: [f32; 3],
        diffuse: [f32; 3],
        emissive: [f32; 3],
        opacity: f32,
        alpha_mode: AlphaMode,
        double_sided: bool,
    ) -> Self {
        Self {
            parameters: Parameters {
                ambient: Vec3::from_array(ambient).extend(0.0),
                diffuse: Vec3::from_array(diffuse).extend(opacity),
                emissive: Vec3::from_array(emissive).extend(0.0),
                scene_ambient: Vec4::ZERO,
                point_position: [Vec4::ZERO; 4],
                point_ambient: [Vec4::ZERO; 4],
                terrain_lights: 0,
                light_color: [Vec4::ZERO; 4],
                light_direction: [Vec4::ZERO; 4],
                vertex_diffuse: 0,
                opacity: 1.0,
                primary_gradient: 1,
                shroud_extent: Vec4::ZERO,
                alpha_test: if matches!(alpha_mode, AlphaMode::Mask(_)) {
                    96.0 / 255.0
                } else {
                    -1.0
                },
            },
            texture: None,
            shroud: None,
            alpha_mode,
            force_sorted_phase: false,
            double_sided,
            depth_bias: 0.0,
            depth_compare: None,
            depth_write: matches!(alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)),
        }
    }
}

// ShaderClass presets use identical source/destination factors for RGBA.
const fn blend_component(source: BlendFactor) -> BlendComponent {
    BlendComponent {
        src_factor: source,
        dst_factor: match source {
            BlendFactor::Zero => BlendFactor::Src,
            BlendFactor::One => BlendFactor::One,
            _ => BlendFactor::OneMinusSrcAlpha,
        },
        operation: BlendOperation::Add,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplicative_preset_uses_source_rgb_as_destination_factor_without_alpha_blending() {
        let material = SourceMaterial::new(
            [0.0; 3],
            [0.0; 3],
            [0.0; 3],
            1.0,
            AlphaMode::Multiply,
            false,
        );
        let source = MaterialKey::from(&material).blend_source.unwrap();
        let blend = blend_component(source);
        assert_eq!(blend.src_factor, BlendFactor::Zero);
        assert_eq!(blend.dst_factor, BlendFactor::Src);
        assert_eq!(blend.operation, BlendOperation::Add);
        assert!(!material.depth_write);
        assert!(material.parameters.alpha_test < 0.0);
        assert_eq!(
            blend_component(BlendFactor::One).dst_factor,
            BlendFactor::One
        );
        assert_eq!(
            blend_component(BlendFactor::SrcAlpha).dst_factor,
            BlendFactor::OneMinusSrcAlpha
        );
    }
}
