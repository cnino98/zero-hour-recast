use super::{Emitter, Particles, Shader, Style, Template, streak, volume};
use crate::{playtest::Match, playtest_view::WorldCamera, source_material::SourceMaterial};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};

#[cfg(test)]
mod equivalence;
mod geometry;
#[cfg(test)]
mod upload_tests;
#[cfg(test)]
mod upload_usage_tests;

#[derive(Component)]
pub struct ParticleVisual {
    pub system: u64,
    pub generation: u32,
    pub(crate) bounded: bool,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    mut particles: ResMut<Particles>,
    time: Res<Time<Fixed>>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut resources: RenderResources,
    visuals: Query<(Entity, &ParticleVisual, &Mesh3d)>,
) {
    let _span = resources
        .timings
        .as_deref()
        .map(|timings| timings.span("particles"));
    let battle = battle.into_inner();
    let camera = camera.into_inner();
    let time = time.into_inner();
    let phase = if battle.started {
        time.overstep_fraction().clamp(0.0, 1.0)
    } else {
        0.0
    };
    if phase > particles.phase {
        let step = phase - particles.phase;
        particles.draw(step);
        particles.phase = phase;
    }
    upload_textures(&mut particles, &mut resources.images);
    let inverse = camera.compute_affine().inverse();
    let world = camera.compute_affine();
    let active: std::collections::BTreeSet<_> = particles
        .emitters
        .iter()
        .filter(|emitter| !emitter.particles.is_empty())
        .map(|emitter| emitter.id)
        .collect();
    let mut existing = std::collections::BTreeMap::new();
    for (entity, visual, mesh) in &visuals {
        if !battle.started
            || visual.generation != battle.generation
            || !active.contains(&visual.system)
        {
            commands.entity(entity).despawn();
            resources.meshes.remove(&mesh.0);
        } else {
            existing.insert(visual.system, (entity, mesh.0.clone()));
        }
    }
    if !battle.started {
        return;
    }
    upload_materials(&mut particles, &mut resources.materials);
    let mut reported = std::mem::take(&mut particles.reported_streak);
    for emitter in &particles.emitters {
        if emitter.particles.is_empty() {
            continue;
        }
        let template = &particles.templates[emitter.template];
        if matches!(template.drawing.style, Style::Smudge) {
            continue;
        }
        let result = mesh(emitter, template, inverse, world);
        let mesh = match result {
            Ok(mesh) => mesh,
            Err(error) => {
                if reported.insert(emitter.template) {
                    warn!("Particle {}: {error}", template.name);
                }
                if let Some((entity, handle)) = existing.get(&emitter.id) {
                    commands.entity(*entity).despawn();
                    resources.meshes.remove(handle);
                }
                continue;
            }
        };
        let bounded =
            matches!(template.drawing.style, Style::Streak) && emitter.particles.len() > 1;
        let visual = ParticleVisual {
            system: emitter.id,
            generation: battle.generation,
            bounded,
        };
        let layer = RenderLayers::layer(render_layer(&template.drawing, emitter.particles.len()));
        if let Some((entity, handle)) = existing.get(&emitter.id) {
            commands.entity(*entity).insert((visual, layer));
            geometry::update(&mut resources.meshes, handle, mesh);
        } else {
            let material = particles.textures[template.texture].materials
                [template.drawing.shader.index()]
            .as_ref()
            .expect("active particle material uploaded")
            .clone();
            commands.spawn((
                visual,
                Mesh3d(resources.meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::IDENTITY,
                layer,
                NoFrustumCulling,
            ));
        }
    }
    particles.reported_streak = reported;
}

// PointGroupClass::Render / RenderVolumeParticle and SegLineRenderer::Render.
const fn render_layer(drawing: &super::Drawing, points: usize) -> usize {
    let line = matches!(drawing.style, Style::Streak) && points > 1;
    let volume = matches!(drawing.style, Style::Volume(depth) if depth > 1);
    if matches!(drawing.shader, Shader::AlphaTest) || drawing.ground_aligned && !line && !volume {
        7
    } else {
        3
    }
}

fn upload_materials(particles: &mut Particles, materials: &mut Assets<SourceMaterial>) {
    for emitter in &particles.emitters {
        if emitter.particles.is_empty() {
            continue;
        }
        let template = &particles.templates[emitter.template];
        if matches!(template.drawing.style, Style::Smudge) {
            continue;
        }
        let texture = &mut particles.textures[template.texture];
        let slot = &mut texture.materials[template.drawing.shader.index()];
        if slot.is_some() {
            continue;
        }
        let mut material = SourceMaterial::new(
            [0.0; 3],
            [0.0; 3],
            [0.0; 3],
            1.0,
            match template.drawing.shader {
                Shader::Alpha => AlphaMode::Blend,
                Shader::Additive => AlphaMode::Add,
                Shader::AlphaTest => AlphaMode::Mask(96.0 / 255.0),
            },
            !matches!(template.drawing.shader, Shader::AlphaTest),
        );
        material.parameters.vertex_diffuse = 3;
        if matches!(template.drawing.shader, Shader::AlphaTest) {
            // ShaderClass::_PresetATestSpriteShader selects texture RGBA alone.
            material.parameters.primary_gradient = 0;
        }
        material.texture.clone_from(&texture.handle);
        *slot = Some(materials.add(material));
    }
}

fn upload_textures(particles: &mut Particles, images: &mut Assets<Image>) {
    for texture in &mut particles.textures {
        if let Some(mut image) = texture.image.take() {
            image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
            texture.handle = Some(images.add(image));
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct RenderResources<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    timings: Option<Res<'w, crate::render_performance::Timings>>,
}

fn source(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[2], -value[1]) * 0.1
}

pub(super) fn mesh(
    emitter: &Emitter,
    template: &Template,
    inverse: bevy::math::Affine3A,
    world: bevy::math::Affine3A,
) -> Result<Mesh, &'static str> {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut uv = Vec::new();
    let mut indices = Vec::new();
    if matches!(template.drawing.style, Style::Streak) && emitter.particles.len() > 1 {
        let mut start = 0;
        // Native STREAK_CHUNK_SIZE=128 with one shared endpoint between chunks.
        while start + 1 < emitter.particles.len() {
            let end = (start + 129).min(emitter.particles.len());
            let part = &emitter.particles[start..end];
            let mut points: Vec<_> = part
                .iter()
                .map(|p| inverse.transform_point3(source(p.position)))
                .collect();
            // Native chunking offsets positions only: widths, personalities and
            // output colors continue indexing the beginning of their arrays.
            let attributes = &emitter.particles[..part.len()];
            let widths: Vec<_> = attributes.iter().map(|p| p.size * 0.1).collect();
            let personalities: Vec<_> = attributes.iter().map(|p| p.personality).collect();
            let strip = streak::geometry(&mut points, &widths, &personalities)?;
            let base = u32::try_from(positions.len()).map_err(|_| "particle mesh exceeds u32")?;
            for (i, point) in strip.vertices.into_iter().enumerate() {
                positions.push(world.transform_point3(point).to_array());
                let p = &attributes[(i / 2).min(attributes.len() - 1)];
                colors.push(if i / 2 == 0 {
                    [0.0; 4]
                } else {
                    rgba(p.color, p.alpha)
                });
            }
            uv.extend(strip.uv);
            indices.extend(strip.indices.into_iter().map(|i| i + base));
            start = end - 1;
        }
    } else {
        let (depth, points) = match template.drawing.style {
            Style::Volume(depth) => (
                depth,
                &emitter.particles[..emitter.particles.len().min(512)],
            ),
            _ => (1, emitter.particles.as_slice()),
        };
        let camera = [
            world.translation.x * 10.0,
            -world.translation.z * 10.0,
            world.translation.y * 10.0,
        ];
        for layer in 0..depth {
            for p in points {
                let position = if depth > 1 && !template.drawing.ground_aligned {
                    volume::position(p.position, camera, points[0].size, depth, layer)
                } else {
                    p.position
                };
                let base =
                    u32::try_from(positions.len()).map_err(|_| "particle mesh exceeds u32")?;
                positions.extend(quad(
                    p,
                    position,
                    template.drawing.ground_aligned,
                    inverse,
                    world,
                ));
                colors.extend([rgba(p.color, p.alpha); 4]);
                uv.extend([[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
                indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        // The shared SortingRenderer pool orders all volume triangles together
        // with other emitters, laser segments and sorted model surfaces.
    }
    let count = positions.len();
    // Layer 3 is consumed on the CPU by the shared SortingRenderer pool;
    // only its generated runs draw. Direct layer 7 still needs GPU geometry.
    let usage = if render_layer(&template.drawing, emitter.particles.len()) == 3 {
        RenderAssetUsages::MAIN_WORLD
    } else {
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD
    };
    Ok(Mesh::new(PrimitiveTopology::TriangleList, usage)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices)))
}

fn quad(
    particle: &super::Particle,
    position: [f32; 3],
    ground_aligned: bool,
    inverse: bevy::math::Affine3A,
    world: bevy::math::Affine3A,
) -> [[f32; 3]; 4] {
    if ground_aligned {
        return ground_quad(position, particle.angle, particle.size)
            .map(|point| source(point).to_array());
    }
    let center = inverse.transform_point3(source(position));
    let (sine, cosine) = orientation(particle.angle);
    [[-0.5, 0.5], [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5]].map(|[x, y]| {
        let xc = x * cosine;
        let ys = y * sine;
        let xs = x * sine;
        let yc = y * cosine;
        let offset = Vec3::new(xc - ys, xs + yc, 0.0) * (particle.size * 0.1);
        world.transform_point3(center + offset).to_array()
    })
}

// PointGroupClass::QUADS_SIZE_ORIENT, nonbillboard branch. D3DX's row-vector
// rotation is read through WWMath's column-vector multiply, reversing Z rotation.
// Native ground quads have twice the billboard width and decode with /255.
fn ground_quad(position: [f32; 3], angle: f32, radius: f32) -> [[f32; 3]; 4] {
    let encoded = (angle * 255.0 / std::f32::consts::TAU)
        .trunc()
        .rem_euclid(256.0);
    let radians = encoded / 255.0 * 2.0 * std::f32::consts::PI;
    let (sine, cosine) = radians.sin_cos();
    let x = [cosine, -sine];
    let y = [sine, cosine];
    [
        [x[0] + y[0], x[1] + y[1]],
        [x[0] - y[0], x[1] - y[1]],
        [-(x[0] + y[0]), -(x[1] + y[1])],
        [-x[0] + y[0], -x[1] + y[1]],
    ]
    .map(|offset| {
        let scaled = offset.map(|component| component * radius);
        [
            position[0] + scaled[0],
            position[1] + scaled[1],
            position[2],
        ]
    })
}

fn rgba(color: [f32; 3], alpha: f32) -> [f32; 4] {
    let c = color.map(|v| (v.clamp(0.0, 1.0) * 255.0).trunc() / 255.0);
    [
        c[0],
        c[1],
        c[2],
        (alpha.clamp(0.0, 1.0) * 255.0).trunc() / 255.0,
    ]
}

// PointGroupClass::_Init builds one table by repeated float addition. Keep
// that order (angle * step would round differently), including the old
// non-finite fallback entry; finite simulation angles select entries 0..255.
static ORIENTATIONS: std::sync::LazyLock<[(f32, (f32, f32)); 257]> =
    std::sync::LazyLock::new(|| {
        let mut radians: f32 = 0.0;
        let step = std::f32::consts::TAU / 256.0;
        std::array::from_fn(|index| {
            let entry = (
                f32::from(u16::try_from(index).expect("orientation table index")),
                radians.sin_cos(),
            );
            radians += step;
            entry
        })
    });

fn orientation(angle: f32) -> (f32, f32) {
    let encoded = (angle * 255.0 / std::f32::consts::TAU)
        .trunc()
        .rem_euclid(256.0);
    let index = if encoded.is_nan() {
        256
    } else {
        ORIENTATIONS.partition_point(|&(index, _)| index < encoded)
    };
    ORIENTATIONS[index].1
}

#[cfg(test)]
mod tests {
    use super::orientation;

    #[test]
    fn ground_quads_keep_original_full_radius_and_world_plane_when_camera_rotates() {
        use super::{ground_quad, mesh};
        use crate::particles::{
            Pose,
            tests::{system, template},
        };
        use bevy::{math::Affine3A, mesh::VertexAttributeValues, prelude::*};
        let expected = [
            [15.0, 25.0, 8.0],
            [15.0, 15.0, 8.0],
            [5.0, 15.0, 8.0],
            [5.0, 25.0, 8.0_f32],
        ];
        assert_eq!(
            ground_quad([10.0, 20.0, 8.0], 0.0, 5.0).map(|point| point.map(f32::to_bits)),
            expected.map(|point| point.map(f32::to_bits))
        );
        let mut rules = template("ground");
        rules.drawing.ground_aligned = true;
        let mut particles = system(vec![rules]);
        particles.create(0, Pose::at([10.0, 20.0, 8.0]), None, &mut |_, high| high);
        particles.logic(0, &mut |_, high| high, &|_| 0.0);
        let emitter = &particles.emitters[0];
        let rules = &particles.templates[0];
        let first = mesh(emitter, rules, Affine3A::IDENTITY, Affine3A::IDENTITY).unwrap();
        let camera = Affine3A::from_rotation_translation(
            Quat::from_euler(EulerRot::YXZ, 0.4, -0.7, 0.2),
            Vec3::new(5.0, 80.0, 40.0),
        );
        let second = mesh(emitter, rules, camera.inverse(), camera).unwrap();
        let Some(VertexAttributeValues::Float32x3(a)) = first.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let Some(VertexAttributeValues::Float32x3(b)) = second.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        assert_eq!(
            a.iter()
                .map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>(),
            b.iter()
                .map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>()
        );
        assert!(
            a.iter()
                .all(|point| point[1].to_bits() == (8.0_f32 * 0.1).to_bits())
        );
    }

    #[test]
    fn native_table_matches_repeated_float_steps_at_wrapped_quantization_edges() {
        for angle in (-2048..=2048_i16)
            .map(|value| f32::from(value) * std::f32::consts::TAU / 1020.0)
            .chain([f32::NAN, f32::INFINITY, f32::NEG_INFINITY])
        {
            let encoded = (angle * 255.0 / std::f32::consts::TAU)
                .trunc()
                .rem_euclid(256.0);
            let mut radians: f32 = 0.0;
            for index in 0..256_u16 {
                if f32::from(index) >= encoded {
                    break;
                }
                radians += std::f32::consts::TAU / 256.0;
            }
            let expected = radians.sin_cos();
            let actual = orientation(angle);
            assert_eq!(
                (actual.0.to_bits(), actual.1.to_bits()),
                (expected.0.to_bits(), expected.1.to_bits())
            );
        }
    }
}

#[cfg(test)]
mod sorting_tests {
    use super::*;
    #[test]
    fn source_ground_point_groups_stay_immediate_but_real_streaks_and_volumes_sort() {
        let mut drawing = super::super::Drawing {
            style: Style::Billboard,
            shader: Shader::Alpha,
            ground_aligned: true,
        };
        assert_eq!(render_layer(&drawing, 4), 7);
        drawing.style = Style::Volume(1);
        assert_eq!(render_layer(&drawing, 4), 7);
        drawing.style = Style::Volume(2);
        assert_eq!(render_layer(&drawing, 4), 3);
        drawing.style = Style::Streak;
        assert_eq!(render_layer(&drawing, 1), 7);
        assert_eq!(render_layer(&drawing, 2), 3);
        drawing.shader = Shader::AlphaTest;
        assert_eq!(render_layer(&drawing, 2), 7);
        drawing.style = Style::Billboard;
        drawing.ground_aligned = false;
        drawing.shader = Shader::Additive;
        assert_eq!(render_layer(&drawing, 1), 3);
    }
}
