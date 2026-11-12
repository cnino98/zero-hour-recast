//! Native `PointGroup` quad size, encoded texture color and dynamic buffer bounds.
pub mod verification;
use super::{Emitters, buffer::Buffer, pose::Pose};
use crate::{
    model_animation::Clock, playtest::Match, playtest_view::WorldCamera,
    source_material::SourceMaterial,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::{primitives::Aabb, visibility::RenderLayers},
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use std::collections::BTreeMap;

#[derive(Component)]
pub struct EmitterVisual {
    id: u64,
    generation: u32,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Resources<'w> {
    images: ResMut<'w, Assets<Image>>,
    materials: ResMut<'w, Assets<SourceMaterial>>,
    meshes: ResMut<'w, Assets<Mesh>>,
}

pub fn update(
    mut commands: Commands,
    battle: Res<Match>,
    clock: Res<Clock>,
    emitters: Option<ResMut<Emitters>>,
    camera: Single<&Transform, With<WorldCamera>>,
    mut resources: Resources,
    visuals: Query<(Entity, &EmitterVisual, &Mesh3d)>,
) {
    let battle = battle.into_inner();
    let clock = clock.into_inner();
    let camera = camera.into_inner();
    let Some(mut emitters) = emitters else {
        return;
    };
    if battle.started {
        emitters.scene.update(
            clock.committed_milliseconds(),
            clock.frame_milliseconds(),
            |object| {
                let actor = battle.simulation.entities.get(object)?;
                let (position, basis) = battle.simulation.object_transform(object);
                Some((
                    Pose::new(position, basis),
                    actor.drawable_effectively_hidden(),
                ))
            },
        );
    }
    let mut existing = BTreeMap::new();
    for (entity, visual, mesh) in &visuals {
        if !battle.started
            || visual.generation != battle.generation
            || !emitters
                .scene
                .emitters
                .iter()
                .any(|emitter| emitter.id == visual.id && !emitter.buffer.particles.is_empty())
        {
            commands.entity(entity).despawn();
            resources.meshes.remove(&mesh.0);
        } else {
            existing.insert(visual.id, (entity, mesh.0.clone()));
        }
    }
    if !battle.started {
        return;
    }
    upload_materials(&mut emitters, &mut resources);
    let right = camera.rotation * Vec3::X;
    let up = camera.rotation * Vec3::Y;
    for emitter in &emitters.scene.emitters {
        let Some(bounds) = emitter.buffer.bounds() else {
            continue;
        };
        let mesh = geometry(&emitter.buffer, clock.committed_milliseconds(), right, up);
        let first = source(bounds.0);
        let last = source(bounds.1);
        let bounds = Aabb::from_min_max(first.min(last), first.max(last));
        if let Some((entity, handle)) = existing.get(&emitter.id) {
            *resources
                .meshes
                .get_mut(handle)
                .expect("existing emitter mesh") = mesh;
            commands.entity(*entity).insert(bounds);
        } else {
            let binding = emitters.definitions.bindings[emitter.template]
                .as_ref()
                .expect("bound emitter");
            let material = emitters.textures[binding.texture]
                .material
                .clone()
                .expect("uploaded emitter texture");
            commands.spawn((
                EmitterVisual {
                    id: emitter.id,
                    generation: battle.generation,
                },
                Mesh3d(resources.meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::IDENTITY,
                bounds,
                RenderLayers::layer(7),
            ));
        }
    }
}

fn upload_materials(emitters: &mut Emitters, resources: &mut Resources<'_>) {
    for texture in &mut emitters.textures {
        let Some(image) = texture.image.take() else {
            continue;
        };
        let image = resources.images.add(image);
        let mut material = SourceMaterial::new(
            [0.0; 3],
            [0.0; 3],
            [0.0; 3],
            1.0,
            AlphaMode::Mask(96.0 / 255.0),
            false,
        );
        // Native emitter shader enables alpha-test/depth-write and disables the
        // primary gradient: texture RGBA is used without a particle color factor.
        material.parameters.primary_gradient = 0;
        material.parameters.vertex_diffuse = 3;
        material.texture = Some(image);
        texture.material = Some(resources.materials.add(material));
    }
}

pub(super) fn geometry(buffer: &Buffer, now: u32, right: Vec3, up: Vec3) -> Mesh {
    let mut positions = Vec::with_capacity(buffer.particles.len() * 4);
    let mut uv = Vec::with_capacity(buffer.particles.len() * 4);
    let mut indices = Vec::with_capacity(buffer.particles.len() * 6);
    for particle in &buffer.particles {
        let base = u32::try_from(positions.len()).expect("validated emitter mesh size");
        let center = source(particle.position);
        let size = buffer.size(particle, now) * 0.1;
        for [x, y] in [[-0.5, 0.5], [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5]] {
            let horizontal = right * (x * size);
            let vertical = up * (y * size);
            positions.push((center + horizontal + vertical).to_array());
        }
        uv.extend([[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0; 4]; count])
    .with_inserted_indices(Indices::U32(indices))
}

fn source(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[2], -value[1]) * 0.1
}
