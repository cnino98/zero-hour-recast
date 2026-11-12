//! Actual scene-sampling fans for fixed-camera native pixel review.
use super::{ParticleInspection, Style};
use bevy::{mesh::VertexAttributeValues, prelude::*};
use std::error::Error;

pub struct Fan {
    pub bounds: Rect,
    pub displacement: [f32; 2],
    pub alpha: f32,
}

impl ParticleInspection<'_, '_> {
    pub fn smudge_fans(
        &self,
        name: &str,
        generation: u32,
        camera: (&Camera, &GlobalTransform),
    ) -> Result<Vec<Fan>, Box<dyn Error>> {
        let mut fans = Vec::new();
        for emitter in &self.particles.emitters {
            let template = &self.particles.templates[emitter.template];
            if template.name != name || !matches!(template.drawing.style, Style::Smudge) {
                continue;
            }
            let Some((mesh, range)) = self.smudge_mesh(emitter.id, generation) else {
                continue;
            };
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                return Err("smudge fan lacks positions".into());
            };
            let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                return Err("smudge fan lacks sampled UVs".into());
            };
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                return Err("smudge fan lacks opacity".into());
            };
            for first in range.step_by(5) {
                let mut bounds = Rect {
                    min: Vec2::splat(f32::INFINITY),
                    max: Vec2::splat(f32::NEG_INFINITY),
                };
                for point in &points[first..first + 5] {
                    let projected = camera
                        .0
                        .world_to_viewport(camera.1, Vec3::from_array(*point))?;
                    bounds.min = bounds.min.min(projected);
                    bounds.max = bounds.max.max(projected);
                }
                // Compare the actual center sample with the undistorted fan
                // center; the authored random displacement is not inferred.
                let displacement = std::array::from_fn(|axis| {
                    let sum = uv[first][axis] + uv[first + 2][axis];
                    let center = sum * 0.5;
                    uv[first + 4][axis] - center
                });
                fans.push(Fan {
                    bounds,
                    displacement,
                    alpha: colors[first + 4][3],
                });
            }
        }
        Ok(fans)
    }
}
