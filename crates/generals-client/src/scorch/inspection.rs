use crate::{particles::Particles, source_material::SourceMaterial};
use bevy::{ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

#[derive(SystemParam)]
pub struct Inspection<'w, 's> {
    particles: Res<'w, Particles>,
    visuals: Query<
        'w,
        's,
        (
            &'static super::render::Visual,
            &'static MeshMaterial3d<SourceMaterial>,
            &'static ViewVisibility,
        ),
    >,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<SourceMaterial>>,
    images: Res<'w, Assets<Image>>,
}

impl Inspection<'_, '_> {
    pub(crate) fn disarm(&self, position: [f32; 3]) -> Result<bool, Box<dyn Error>> {
        self.at(position, 10.0)
    }

    pub(crate) fn explosion(&self, position: [f32; 3]) -> Result<bool, Box<dyn Error>> {
        self.at(position, 15.0)
    }

    fn at(&self, position: [f32; 3], radius: f32) -> Result<bool, Box<dyn Error>> {
        let data = &self.particles.scorches;
        if !data.marks().any(|mark| {
            mark.position.map(f32::to_bits) == position.map(f32::to_bits)
                && mark.radius.to_bits() == radius.to_bits()
                && (0..4).contains(&mark.kind)
        }) {
            return Ok(false);
        }
        for (visual, material, visible) in &self.visuals {
            if !visible.get() {
                continue;
            }
            let material = self
                .materials
                .get(&material.0)
                .ok_or("missing scorch source material")?;
            if material.texture.as_ref() != data.texture.as_ref()
                || material.parameters.vertex_diffuse != 4
                || !matches!(material.alpha_mode, AlphaMode::Blend)
                || material.depth_write
            {
                return Err(
                    "scorch differs from original atlas/prelit alpha/shroud rendering".into(),
                );
            }
            let texture = data
                .texture
                .as_ref()
                .and_then(|texture| self.images.get(texture))
                .ok_or("original EXScorch01 atlas is not uploaded")?;
            if texture.width() == 0 || texture.texture_descriptor.format.is_srgb() {
                return Err("scorch atlas lost its original fixed-function color space".into());
            }
            let mesh = self
                .meshes
                .get(&visual.mesh)
                .ok_or("missing scorch geometry")?;
            if mesh.indices().is_none_or(bevy::mesh::Indices::is_empty) {
                return Ok(false);
            }
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("missing scorch source positions".into());
            };
            let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                return Err("missing scorch atlas UVs".into());
            };
            if points
                .iter()
                .flatten()
                .chain(uv.iter().flatten())
                .any(|value| !value.is_finite())
            {
                return Err("scorch mesh contains nonfinite source geometry".into());
            }
            return Ok(true);
        }
        Ok(false)
    }
}
