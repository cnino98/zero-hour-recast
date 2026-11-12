//! Rendered source quads for additive pulse pixel review.
use super::{ParticleInspection, Style};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

pub struct Quad {
    pub system: u64,
    pub born: u64,
    pub ordinal: usize,
    pub size: f32,
    pub bounds: Rect,
    pub color: [f32; 4],
}

impl ParticleInspection<'_, '_> {
    pub fn additive_quads(
        &self,
        name: &str,
        generation: u32,
        camera: (&Camera, &GlobalTransform),
    ) -> Result<Vec<Quad>, Box<dyn Error>> {
        let mut result = Vec::new();
        for emitter in &self.particles.emitters {
            let template = &self.particles.templates[emitter.template];
            if template.name != name || !self.visible_mesh(emitter, generation)? {
                continue;
            }
            if !matches!(template.drawing.shader, super::super::Shader::Additive)
                || !matches!(template.drawing.style, Style::Billboard)
            {
                return Err("additive pulse requires its original additive particle quads".into());
            }
            let (_, mesh, _, _, transform) = self
                .visuals
                .iter()
                .find(|(visual, ..)| visual.system == emitter.id && visual.generation == generation)
                .ok_or("additive pulse drawable disappeared")?;
            let mesh = self
                .meshes
                .get(&mesh.0)
                .ok_or("additive pulse mesh disappeared")?;
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("additive pulse lacks positions".into());
            };
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                return Err("additive pulse lacks source colors".into());
            };
            if points.len() != emitter.particles.len() * 4 {
                return Err("additive pulse lost its original quad ordering".into());
            }
            for (index, particle) in emitter.particles.iter().enumerate() {
                let mut bounds = Rect {
                    min: Vec2::splat(f32::INFINITY),
                    max: Vec2::splat(f32::NEG_INFINITY),
                };
                for point in &points[index * 4..index * 4 + 4] {
                    let projected = camera.0.world_to_viewport(
                        camera.1,
                        transform.transform_point(Vec3::from_array(*point)),
                    )?;
                    bounds.min = bounds.min.min(projected);
                    bounds.max = bounds.max.max(projected);
                }
                let ordinal = emitter.particles[..index]
                    .iter()
                    .filter(|earlier| earlier.born == particle.born)
                    .count();
                result.push(Quad {
                    system: emitter.id,
                    born: particle.born,
                    ordinal,
                    size: particle.size,
                    bounds,
                    color: colors[index * 4],
                });
            }
        }
        Ok(result)
    }
}
