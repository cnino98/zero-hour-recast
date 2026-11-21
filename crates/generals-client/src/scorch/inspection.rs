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

    pub(crate) fn at(&self, position: [f32; 3], radius: f32) -> Result<bool, Box<dyn Error>> {
        let data = &self.particles.scorches;
        let Some(mark) = data.marks().find(|mark| {
            mark.position.map(f32::to_bits) == position.map(f32::to_bits)
                && mark.radius.to_bits() == radius.to_bits()
                && mark.radius > 0.0
                && (0..4).contains(&mark.kind)
        }) else {
            return Ok(false);
        };
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
            let Some(indices) = mesh.indices().filter(|indices| !indices.is_empty()) else {
                continue;
            };
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
            if has_mark_triangle(mark, indices, points, uv) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn has_mark_triangle(
    mark: &super::Mark,
    indices: &bevy::mesh::Indices,
    points: &[[f32; 3]],
    uv: &[[f32; 2]],
) -> bool {
    let atlas = [mark.kind % 3, mark.kind / 3]
        .map(|value| f32::from(u16::try_from(value).expect("scorch atlas index")) * 1.5);
    let belongs = |index: usize| {
        let position = [points[index][0], -points[index][2]].map(|value| value.round() * 10.0);
        let offset = std::array::from_fn::<_, 2, _>(|axis| position[axis] - mark.position[axis]);
        let expected = std::array::from_fn(|axis| {
            (atlas[axis] + 0.5 + offset[axis] / (2.0 * mark.radius)) / 4.0
        });
        offset
            .into_iter()
            .all(|distance| distance.abs() < mark.radius + 10.0)
            && uv[index].map(f32::to_bits) == expected.map(f32::to_bits)
    };
    let mut indices = indices.iter();
    while let (Some(first), Some(second), Some(third)) =
        (indices.next(), indices.next(), indices.next())
    {
        if first != second
            && second != third
            && third != first
            && [first, second, third].into_iter().all(belongs)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::Mark;
    use super::has_mark_triangle;
    use bevy::mesh::Indices;

    #[test]
    fn only_indexed_nondegenerate_source_triangles_prove_mark_geometry() {
        let mark = Mark {
            position: [100.0, 100.0, 0.0],
            radius: 10.0,
            kind: 0,
        };
        let points = [[10.0, 0.0, -10.0], [11.0, 0.0, -10.0], [10.0, 0.0, -11.0]];
        let uv = [[0.125, 0.125], [0.25, 0.125], [0.125, 0.25]];
        assert!(!has_mark_triangle(
            &mark,
            &Indices::U16(vec![0, 0, 0]),
            &points,
            &uv,
        ));
        assert!(has_mark_triangle(
            &mark,
            &Indices::U16(vec![0, 1, 2]),
            &points,
            &uv,
        ));
    }
}
