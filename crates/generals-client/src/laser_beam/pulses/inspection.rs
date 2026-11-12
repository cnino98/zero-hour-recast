use super::{PulseVisual, Pulses, Template, tile_factor};
use crate::{
    control_bar::UiData,
    laser_beam::Lasers,
    particles::smudge::SceneMaterial,
    playtest::Match,
    playtest_ui::{PromotionTransition, captures_pointer},
    playtest_view::WorldCamera,
};
use bevy::{mesh::VertexAttributeValues, prelude::*, window::PrimaryWindow};
use std::error::Error;

#[derive(bevy::ecs::system::SystemParam)]
pub struct PulseInspection<'w, 's> {
    battle: Res<'w, Match>,
    lasers: Res<'w, Lasers>,
    ui: Res<'w, UiData>,
    promotions: Res<'w, PromotionTransition>,
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<WorldCamera>>,
    meshes: Res<'w, Assets<Mesh>>,
    images: Res<'w, Assets<Image>>,
    materials: Res<'w, Assets<SceneMaterial>>,
    visuals: Query<
        'w,
        's,
        (
            &'static PulseVisual,
            &'static Mesh3d,
            &'static MeshMaterial3d<SceneMaterial>,
            &'static ViewVisibility,
        ),
    >,
}

impl PulseInspection<'_, '_> {
    pub fn visible(&self, source: usize, weapon: &str) -> Result<bool, Box<dyn Error>> {
        let Self {
            battle,
            lasers,
            ui,
            promotions,
            window,
            camera,
            meshes,
            images,
            materials,
            visuals,
        } = self;
        let pulses: &Pulses = &lasers.pulses;
        let (camera, transform) = **camera;
        let Some(viewport) = camera.logical_viewport_rect() else {
            return Ok(false);
        };
        for pulse in pulses
            .active
            .iter()
            .filter(|pulse| pulse.source == source && pulse.weapon == weapon)
        {
            if pulse.points.into_iter().any(|point| {
                let point = Vec3::new(point[0], point[2], -point[1]) * 0.1;
                !camera
                    .world_to_viewport(transform, point)
                    .is_ok_and(|position| {
                        viewport.contains(position)
                            && !captures_pointer(battle, window, position, ui, promotions)
                    })
            }) {
                continue;
            }
            for (visual, mesh, material, visible) in visuals {
                if visual.generation != battle.generation
                    || visual.serial != pulse.serial
                    || !visible.get()
                {
                    continue;
                }
                let (Some(mesh), Some(material)) =
                    (meshes.get(&mesh.0), materials.get(&material.0))
                else {
                    continue;
                };
                let template = &pulses.templates[pulses.bindings[&pulse.weapon].template];
                let Some(VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    return Err("weapon laser has no rendered positions".into());
                };
                let Some(VertexAttributeValues::Float32x2(uv)) =
                    mesh.attribute(Mesh::ATTRIBUTE_UV_0)
                else {
                    return Err("weapon laser has no source texture coordinates".into());
                };
                if positions.len() != template.beams.len() * 4
                    || mesh
                        .indices()
                        .is_none_or(|indices| indices.len() != template.beams.len() * 6)
                    || positions.iter().flatten().any(|value| !value.is_finite())
                    || !matches_uvs(template, pulse.points, pulse.offset, uv)
                    || material.smudge != 0
                    || material.texture.as_ref().is_none_or(|texture| {
                        materials
                            .get(template.material.as_ref().expect("loaded pulse material"))
                            .is_none_or(|expected| expected.texture.as_ref() != Some(texture))
                    })
                {
                    return Err("weapon laser lost original layered geometry, authored tiled UVs or source texture material".into());
                }
                if template.name == "MicrowaveDisableStream" {
                    diagnose(
                        battle,
                        pulse,
                        template,
                        mesh,
                        material,
                        images,
                        (camera, transform),
                    );
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn diagnose(
    battle: &Match,
    pulse: &super::Pulse,
    template: &Template,
    mesh: &Mesh,
    material: &SceneMaterial,
    images: &Assets<Image>,
    camera: (&Camera, &GlobalTransform),
) {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    let terrain = pulse
        .points
        .map(|point| battle.simulation.ground_height([point[0], point[1]]));
    let projected: Vec<_> = points
        .iter()
        .map(|point| {
            camera
                .0
                .world_to_viewport(camera.1, Vec3::from_array(*point))
        })
        .collect();
    let indices: Vec<_> = mesh
        .indices()
        .into_iter()
        .flat_map(bevy::mesh::Indices::iter)
        .collect();
    let areas: Vec<_> = indices
        .chunks_exact(3)
        .map(|triangle| {
            let vertices =
                std::array::from_fn::<_, 3, _>(|axis| Vec3::from_array(points[triangle[axis]]));
            (vertices[1] - vertices[0])
                .cross(vertices[2] - vertices[0])
                .length_squared()
        })
        .collect();
    let texture = material
        .texture
        .as_ref()
        .and_then(|handle| images.get(handle));
    let colors: Vec<_> = template
        .beams
        .iter()
        .map(|beam| (beam.width, beam.color))
        .collect();
    println!(
        "Microwave beam diagnostic: weapon={} frame={} points={:?} terrain={terrain:?} vertices={points:?} projected={projected:?} triangle_cross_squared={areas:?} widths_colors={colors:?} uv={:?} tile={} aspect={} offset={} scene_additive=true depth_write=false texture={:?}; CPU assets and ViewVisibility passed, GPU pixel contribution unverified",
        pulse.weapon,
        battle.simulation.tick,
        pulse.points,
        mesh.attribute(Mesh::ATTRIBUTE_UV_0),
        template.tiling,
        template.aspect,
        pulse.offset,
        texture.map(|image| (
            image.width(),
            image.height(),
            image.texture_descriptor.format,
            &image.sampler
        ))
    );
}

fn matches_uvs(template: &Template, points: [[f32; 3]; 2], offset: f32, uv: &[[f32; 2]]) -> bool {
    if uv.len() != template.beams.len() * 4 {
        return false;
    }
    let squared = std::array::from_fn::<_, 3, _>(|axis| {
        let delta = points[1][axis] - points[0][axis];
        delta * delta
    });
    let length = (squared[0] + squared[1] + squared[2]).sqrt();
    template
        .beams
        .iter()
        .zip(uv.chunks_exact(4))
        .all(|(beam, vertices)| {
            let tile = tile_factor(length, beam.width, template.aspect, template.tiling);
            let expected = [
                [0.0, offset],
                [1.0, offset],
                [0.0, tile + offset],
                [1.0, tile + offset],
            ];
            vertices.iter().zip(expected).all(|(actual, expected)| {
                actual.iter().all(|value| value.is_finite())
                    && actual.map(f32::to_bits) == expected.map(f32::to_bits)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::super::pulse_mesh;
    use super::*;

    #[test]
    fn original_positive_microwave_tiling_is_inspected_at_both_beam_endpoints() {
        let mut template = Template {
            name: "MicrowaveDisableStream".into(),
            beams: vec![super::super::super::beam(
                0,
                1,
                [8.0, 8.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
            )],
            particles: ["GenericGreenLaserFlare".into(), "HackFlare".into()],
            lifetime: [4, 4],
            tiling: 0.5,
            scroll: -10.0,
            aspect: 1.0,
            image: None,
            material: None,
        };
        let points = [[0.0, 0.0, 0.0], [80.0, 0.0, 0.0]];
        let offset = 0.375;
        let camera = Transform::from_xyz(0.0, 100.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y);
        let mesh = pulse_mesh(&template, points, offset, &camera).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("microwave pulse lacks original texture coordinates");
        };
        assert_eq!(
            uv,
            &[[0.0, 0.375], [1.0, 0.375], [0.0, 5.375], [1.0, 5.375]]
        );
        assert!(matches_uvs(&template, points, offset, uv));
        let mut corrupted = uv.clone();
        corrupted[2][1] = offset;
        assert!(!matches_uvs(&template, points, offset, &corrupted));
        corrupted = uv.clone();
        corrupted[1][0] = 0.0;
        assert!(!matches_uvs(&template, points, offset, &corrupted));
        assert!(!matches_uvs(&template, points, offset, &uv[..3]));
        // The existing source-negative tiling profile clamps V to the offset.
        template.tiling = -3.0;
        let mesh = pulse_mesh(&template, points, offset, &camera).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("clamped pulse lacks original texture coordinates");
        };
        assert!(
            uv.iter()
                .all(|point| point[1].to_bits() == offset.to_bits())
        );
        assert!(matches_uvs(&template, points, offset, uv));
    }
}
